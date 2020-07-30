use sadmc::system::erfinv::{ParametersN, ErfInv};

use sadmc::mc::energy_transposed::EnergyMC;
use sadmc::mc::MonteCarlo;

fn main() {
    let mut mc = EnergyMC::<ErfInv>::from_args::<ParametersN>();
    loop {
        mc.move_once();
    }
}
