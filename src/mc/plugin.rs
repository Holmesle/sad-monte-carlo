//! A plugin architecture to enable reusing of interfaces and
//! implementation for different Monte Carlo algorithms.

use super::*;

use crate::prettyfloat::PrettyFloat;
use std::cell::Cell;
use std::default::Default;
use std::time;

/// A `Plugin` is an object that can be used to configure a MonteCarlo
/// simulation.  The plugin will be called regularly, and will have a
/// chance to save data (e.g. collect statistics) and/or terminate the
/// simulation.
pub trait Plugin<MC: MonteCarlo> {
    /// Run and do something.  If the simulation needs to be
    /// terminated, `None` is returned.  If you want to modify
    /// information, you will have to use interior mutability, because
    /// I can't figure out any practical way to borrow `self` mutably
    /// while still giving read access to the `MC`.
    fn run(&self, _mc: &MC, _sys: &MC::System) -> Action {
        Action::None
    }
    /// How often we need the plugin to run.  A `None` value means
    /// that this plugin never needs to run.  Note that it is expected
    /// that this period may change any time the plugin is called, so
    /// this should be a cheap call as it may happen frequently.  Also
    /// note that this is an upper, not a lower bound.
    fn run_period(&self) -> TimeToRun {
        TimeToRun::Never
    }
    /// We might be about to die, so please do any cleanup or saving.
    /// Note that the plugin state is stored on each checkpoint.  This
    /// is called in response to `Action::Save` and `Action::Exit`.
    fn save(&self, _mc: &MC, _sys: &MC::System) {}
    /// Log to stdout any interesting data we think our user might
    /// care about.  This is called in response to `Action::Save`,
    /// `Action::Log` and `Action::Exit`.
    fn log(&self, _mc: &MC, _sys: &MC::System) {}
}

/// A time when we want to be run.
#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialOrd, Ord, PartialEq, Eq)]
pub enum TimeToRun {
    /// Don't stop on our behalf!
    Never,
    /// After this many moves in total.
    TotalMoves(u64),
    /// This often.
    Period(u64),
}

/// An action that should be taken based on this plugin's decision.
#[derive(Copy, Clone, Debug, PartialOrd, Ord, PartialEq, Eq)]
pub enum Action {
    /// Nothing special need be done.
    None,
    /// Log interesting information.
    Log,
    /// Save things.
    Save,
    /// Exit the program.
    Exit,
}
impl Action {
    /// Do both of two actions.
    pub fn and(self, other: Action) -> Action {
        ::std::cmp::max(self, other)
    }
}

/// A helper to enable Monte Carlo implementations to easily run their
/// plugins without duplicating code.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PluginManager {
    #[serde(skip, default)]
    period: Cell<u64>,
    #[serde(skip, default)]
    moves: Cell<u64>,
}

impl PluginManager {
    /// Create a plugin manager.
    pub fn new() -> PluginManager {
        PluginManager {
            period: Cell::new(1),
            moves: Cell::new(0),
        }
    }
    /// Run all the plugins, if needed.  This should always be called
    /// with the same set of plugins.  If you want different sets of
    /// plugins, use different managers.
    pub fn run<MC: MonteCarlo>(&self, mc: &MC, sys: &MC::System, plugins: &[&dyn Plugin<MC>]) {
        let moves = self.moves.get() + 1;
        self.moves.set(moves);
        if moves >= self.period.get() {
            self.moves.set(0);
            let mut todo = plugin::Action::None;
            for p in plugins.iter() {
                todo = todo.and(p.run(mc, sys));
            }
            if todo >= plugin::Action::Log {
                sys.verify_energy();
                for p in plugins.iter() {
                    p.log(mc, sys);
                }
            }
            if todo >= plugin::Action::Save {
                let time = time::Instant::now();
                mc.checkpoint();
                for p in plugins.iter() {
                    p.save(mc, sys);
                }
                let saving_time = time.elapsed().as_secs();
                if saving_time > 5 {
                    println!(
                        "        checkpointing took {}",
                        format_duration(saving_time)
                    );
                }
            }
            if todo >= plugin::Action::Exit {
                ::std::process::exit(0);
            }
            // run plugins every trillion iterations minimum
            let mut new_period = 1u64 << 40;
            for p in plugins.iter() {
                match p.run_period() {
                    TimeToRun::Never => (),
                    TimeToRun::TotalMoves(moves) => {
                        if moves > mc.num_moves() && moves - mc.num_moves() < new_period {
                            new_period = moves - mc.num_moves();
                        }
                    }
                    TimeToRun::Period(period) => {
                        if period < new_period {
                            new_period = period;
                        }
                    }
                }
            }
            self.period.set(new_period);
        }
    }
}

/// A plugin that terminates the simulation after a fixed number of iterations.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Report {
    max_iter: TimeToRun,
    #[serde(default)]
    max_independent_samples: Option<u64>,
    /// This is when and where the simulation started.
    #[serde(skip, default)]
    start: Cell<Option<(time::Instant, u64)>>,
    /// The user has requested that nothing be printed!
    pub quiet: bool,
}

/// The parameters to define the report information as well as stop
/// time (which is part of the report).
#[derive(AutoArgs, Debug, Clone)]
pub struct ReportParams {
    /// The maximum number of iterations to run.
    pub max_iter: Option<u64>,
    /// The maximum number of independent samples to find.
    pub max_independent_samples: Option<u64>,
    /// Do not make reports!
    pub quiet: bool,
}

impl Default for ReportParams {
    fn default() -> Self {
        ReportParams {
            max_iter: None,
            max_independent_samples: None,
            quiet: true,
        }
    }
}

impl From<ReportParams> for Report {
    fn from(params: ReportParams) -> Self {
        Report {
            max_iter: if let Some(mi) = params.max_iter {
                TimeToRun::TotalMoves(mi)
            } else {
                TimeToRun::Never
            },
            max_independent_samples: params.max_independent_samples,
            start: Cell::new(Some((time::Instant::now(), 0))),
            quiet: params.quiet,
        }
    }
}
impl Report {
    /// Allows a resuming simulation to get updated report parameters
    /// from the flags.
    pub fn update_from(&mut self, params: ReportParams) {
        let other = Self::from(params);
        self.max_iter = other.max_iter;
        self.max_independent_samples = other.max_independent_samples;
        self.quiet = other.quiet;
    }

    /// Print a log message
    pub fn print(&self, moves: u64, independent_samples: u64) {
        if self.quiet {
            return;
        }
        match self.start.get() {
            Some((start_time, start_iter)) => {
                let runtime = start_time.elapsed();
                let time_per_move = duration_to_secs(runtime) / (moves - start_iter) as f64;
                if let TimeToRun::TotalMoves(max) = self.max_iter {
                    let frac_complete = moves as f64 / max as f64;
                    let moves_left = if max >= moves { max - moves } else { 0 };
                    let time_left = (time_per_move * moves_left as f64) as u64;
                    print!(
                        "[{}] {}% complete after {} ({} left, {:.1}us per move)",
                        PrettyFloat(moves as f64),
                        (100. * frac_complete) as isize,
                        format_duration(runtime.as_secs()),
                        format_duration(time_left),
                        PrettyFloat(time_per_move * 1e6),
                    );
                } else {
                    print!(
                        "[{}] after {} ({:.1}us per move)",
                        PrettyFloat(moves as f64),
                        format_duration(runtime.as_secs()),
                        PrettyFloat(time_per_move * 1e6),
                    );
                }
                if let Some(max) = self.max_independent_samples {
                    let frac_complete = independent_samples as f64 / max as f64;
                    let samples_left = if max >= independent_samples {
                        max - independent_samples
                    } else {
                        0
                    };
                    let moves_per_sample = moves as f64 / (1.0 + independent_samples as f64);
                    let time_left = (time_per_move * samples_left as f64 * moves_per_sample) as u64;
                    let time_per_sample = time_per_move * moves_per_sample;
                    if time_per_sample < 2.0 {
                        println!(
                            "{}% done ({} left, {:.2} s per sample)",
                            (100. * frac_complete) as isize,
                            format_duration(time_left),
                            PrettyFloat(time_per_sample),
                        );
                    } else {
                        println!(
                            "{}% done ({} left, {} per sample)",
                            (100. * frac_complete) as isize,
                            format_duration(time_left),
                            format_duration(time_per_sample as u64),
                        );
                    }
                } else {
                    println!();
                }
            }
            None => {
                self.start.set(Some((time::Instant::now(), moves)));
            }
        }
    }

    /// Am all done?
    pub fn am_all_done(&self, moves: u64, independent_samples: u64) -> bool {
        if let TimeToRun::TotalMoves(maxiter) = self.max_iter {
            if moves >= maxiter {
                return true;
            }
        }
        if let Some(mis) = self.max_independent_samples {
            independent_samples >= mis
        } else {
            false
        }
    }
}
impl<MC: MonteCarlo> Plugin<MC> for Report {
    fn run(&self, mc: &MC, _sys: &MC::System) -> Action {
        if self.am_all_done(mc.num_moves(), mc.independent_samples()) {
            return Action::Exit;
        }
        Action::None
    }
    fn run_period(&self) -> TimeToRun {
        self.max_iter
    }
    fn log(&self, mc: &MC, _sys: &MC::System) {
        self.print(mc.num_moves(), mc.independent_samples());
    }
    fn save(&self, mc: &MC, _sys: &MC::System) {
        if self.quiet {
            return;
        }
        let accepted = mc.num_accepted_moves();
        let moves = mc.num_moves();
        println!(
            "        Accepted {:.2}/{:.2} = {:.0}% of the moves",
            PrettyFloat(accepted as f64),
            PrettyFloat(moves as f64),
            100.0 * accepted as f64 / moves as f64
        );
    }
}

/// A plugin that schedules when to save
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Save {
    #[serde(skip, default)]
    next_output: Cell<u64>,
    /// This is when and where the simulation started.
    #[serde(skip, default)]
    start: Cell<Option<(time::Instant, u64)>>,
    /// How frequently to save...
    #[serde(default)]
    save_time_seconds: Option<f64>,
}

/// The parameter to define the save schedule
#[derive(AutoArgs, Debug, Clone)]
pub struct SaveParams {
    /// Maximum time between saves in hours
    pub save_time: Option<f64>,
}

impl Default for SaveParams {
    fn default() -> Self {
        SaveParams {
            save_time: Some(1.0),
        }
    }
}
impl Default for Save {
    fn default() -> Self {
        Save::from(SaveParams::default())
    }
}
impl From<SaveParams> for Save {
    fn from(params: SaveParams) -> Self {
        Save {
            next_output: Cell::new(1),
            start: Cell::new(Some((time::Instant::now(), 0))),
            save_time_seconds: params.save_time.map(|h| 60. * 60. * h),
        }
    }
}
impl Save {
    /// Allows a resuming simulation to get updated save parameters
    /// from the flags.
    pub fn update_from(&mut self, params: SaveParams) {
        self.save_time_seconds = params.save_time.map(|h| 60. * 60. * h);
    }
    /// Allows to use just the save plugin by itself without the rest of
    /// the plugin infrastructure. Returns true if we should save now.
    pub fn shall_i_save(&self, moves: u64) -> bool {
        let save_please = moves > self.next_output.get();
        if save_please {
            // We are definitely saving now, and will also decide when to save next.
            if let Some(period) = self.save_time_seconds {
                match self.start.get() {
                    Some((start_time, start_iter)) => {
                        let runtime = start_time.elapsed();
                        let time_per_move = duration_to_secs(runtime) / (moves - start_iter) as f64;
                        let moves_per_period = 1 + (period / time_per_move) as u64;
                        if moves_per_period < moves {
                            self.next_output.set(moves + moves_per_period);
                        } else if moves as f64 + 1.0 < 1.0 / time_per_move {
                            self.next_output.set((1.0 / time_per_move) as u64);
                        } else {
                            self.next_output.set(moves * 2);
                        }
                    }
                    None => {
                        self.start.set(Some((time::Instant::now(), moves)));
                        self.next_output.set(moves + (1 << 20));
                    }
                }
            } else {
                self.next_output.set(self.next_output.get() * 2)
            }
        }
        save_please
    }
}
impl<MC: MonteCarlo> Plugin<MC> for Save {
    fn run(&self, mc: &MC, _sys: &MC::System) -> Action {
        if mc.num_moves() >= self.next_output.get() {
            Action::Save
        } else {
            Action::None
        }
    }
    fn run_period(&self) -> TimeToRun {
        TimeToRun::TotalMoves(self.next_output.get())
    }
    fn save(&self, mc: &MC, _sys: &MC::System) {
        self.shall_i_save(mc.num_moves());
    }
}

/// A plugin that schedules movie backups
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Movie {
    movie_time: Option<f64>,
    which_frame: Cell<i32>,
    period: Cell<plugin::TimeToRun>,
}

/// The parameter to define the movie schedule
#[derive(AutoArgs, Debug, Clone)]
pub struct MovieParams {
    /// 2.0 means a frame every time iterations double.
    pub movie_time: Option<f64>,
}

impl Default for MovieParams {
    fn default() -> Self {
        MovieParams { movie_time: None }
    }
}
impl From<MovieParams> for Movie {
    fn from(params: MovieParams) -> Self {
        Movie {
            movie_time: params.movie_time,
            which_frame: Cell::new(0),
            period: Cell::new(if params.movie_time.is_some() {
                plugin::TimeToRun::TotalMoves(1)
            } else {
                plugin::TimeToRun::Never
            }),
        }
    }
}
impl Default for Movie {
    fn default() -> Self {
        Movie::from(MovieParams::default())
    }
}
impl Movie {
    /// Save a frame of the movie.
    pub fn save_frame<MC: serde::Serialize>(&self, save_as: &std::path::Path, moves: u64, mc: &MC) {
        let dir = save_as.with_extension("");
        let dir = std::path::Path::new(&dir);
        let path = dir.join(format!("{:014}.cbor", moves));
        println!("Saving movie as {:?}", path);

        std::fs::create_dir_all(&dir).expect("error creating directory");
        let f = AtomicFile::create(&path).expect(&format!("error creating file {:?}", path));
        serde_cbor::to_writer(&f, mc).expect("error writing movie frame?!");
    }
    /// Is it time for a movie?
    ///
    /// Allows to use just the save plugin by itself without the rest of
    /// the plugin infrastructure. Returns true if we should save a movie frame now.
    pub fn shall_i_save(&self, moves: u64) -> bool {
        if let Some(time) = self.movie_time {
            if plugin::TimeToRun::TotalMoves(moves) == self.period.get() {
                // Now decide when we need the next frame to be.
                let mut which_frame = self.which_frame.get() + 1;
                let mut next_time = (time.powi(which_frame) + 0.5) as u64;
                while next_time <= moves {
                    which_frame += 1;
                    next_time = (time.powi(which_frame) + 0.5) as u64;
                }
                self.which_frame.set(which_frame);
                self.period.set(plugin::TimeToRun::TotalMoves(next_time));
                return true;
            }
        }
        false
    }
}
impl<MC: MonteCarlo> Plugin<MC> for Movie {
    fn run(&self, mc: &MC, _sys: &MC::System) -> Action {
        if self.shall_i_save(mc.num_moves()) {
            // Save movie now.
            self.save_frame(&mc.save_as(), mc.num_moves(), mc);
            return plugin::Action::Save;
        }
        plugin::Action::None
    }
    fn run_period(&self) -> plugin::TimeToRun {
        self.period.get()
    }
}

fn format_duration(secs: u64) -> String {
    let mins = secs / 60;
    let hours = mins / 60;
    let mins = mins % 60;
    let days = hours / 24;
    if days > 15 {
        format!("{} days", (hours / 12 + 1) / 2) // round to nearest day
    } else if days >= 2 {
        match hours % 24 {
            0 => format!("{} days", days),
            1 => format!("{} days 1 hour", days),
            hours => format!("{} days {} hours", days, hours),
        }
    } else if hours > 19 {
        format!("{} hours", hours)
    } else if hours == 0 && mins == 0 {
        format!("{} seconds", secs)
    } else if hours == 0 && mins == 1 && secs == 61 {
        format!("1 minute {} second", secs % 60)
    } else if hours == 0 && mins == 1 {
        format!("1 minute {} seconds", secs % 60)
    } else if hours == 0 {
        format!("{} minutes", mins)
    } else if hours == 1 && mins == 0 {
        format!("1 hour")
    } else if hours == 1 {
        format!("1 hour {} minutes", mins)
    } else if mins == 0 {
        format!("{} hours", hours)
    } else if mins == 1 {
        format!("{} hours 1 minute", hours)
    } else {
        format!("{} hours {} minutes", hours, mins)
    }
}
fn duration_to_secs(t: time::Duration) -> f64 {
    t.as_secs() as f64 + t.subsec_nanos() as f64 * 1e-9
}

#[test]
fn test_format_duration() {
    assert_eq!("5 seconds", format_duration(5).as_str());
    assert_eq!("1 minute 1 second", format_duration(61).as_str());
    assert_eq!("1 minute 2 seconds", format_duration(62).as_str());
    assert_eq!("1 hour", format_duration(60 * 60).as_str());
    assert_eq!("2 hours", format_duration(60 * 60 * 2).as_str());
    assert_eq!(
        "2 hours 1 minute",
        format_duration(60 * 60 * 2 + 1 * 60 + 4).as_str()
    );
    assert_eq!(
        "2 hours 5 minutes",
        format_duration(60 * 60 * 2 + 5 * 60 + 6).as_str()
    );
    assert_eq!("20 hours", format_duration(60 * 60 * 20 + 5 * 60).as_str());
    assert_eq!("24 hours", format_duration(60 * 60 * 24 + 5 * 60).as_str());
    assert_eq!("25 hours", format_duration(60 * 60 * 25 + 5 * 60).as_str());
    assert_eq!("2 days", format_duration(60 * 60 * 48 + 5 * 60).as_str());
    assert_eq!(
        "2 days 1 hour",
        format_duration(60 * 60 * 49 + 5 * 60).as_str()
    );
    assert_eq!(
        "2 days 2 hours",
        format_duration(60 * 60 * (24 * 2 + 2) + 5 * 60).as_str()
    );
    assert_eq!(
        "20 days",
        format_duration(60 * 60 * (24 * 20 + 2) + 5 * 60).as_str()
    );
    assert_eq!(
        "21 days",
        format_duration(60 * 60 * (24 * 20 + 13) + 5 * 60).as_str()
    );
}
