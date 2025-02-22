#!/usr/bin/python3

import numpy as np
from subprocess import run

run(['cargo', 'build', '--release', '--bin',
     'replicas', '--bin', 'histogram'], check=True)

max_iter_default = 1e12


def rq(name, cmd, cpus):
    run(f'rq run -c {cpus} --max-output=30 -R -J'.split() +
        [name, '--']+cmd, check=True)


movie_args = '--movie-time 10^(1/8)'.split()


def run_replicas(name, max_iter=max_iter_default, min_T=0.001, extraname='', extraflags=''):
    save = f'z-{extraname}{name}'
    run('cargo run --bin replicas --release --'.split()+systems[name]+movie_args
        + f'--save-time 0.5 --save-as {save}.cbor'.split()
        + extraflags.split()
        + f'--max-iter {max_iter} --min-T {min_T}'.split(),
        stdout=open(save+'.out', 'a'),
        check=True)


def histogram(name, de, translation_scale):
    return f'../target/release/histogram --save-time 0.5 --energy-bin {de} --translation-scale {translation_scale}'.split()+movie_args+systems[name]


def run_sad(name, de, max_iter=max_iter_default, min_T=0.001, max_E=None, translation_scale=0.05):
    de = str(de)
    save = 'sad-'+name+'-'+de
    max_E_args = []
    if max_E is not None:
        max_E_args = f'--max-allowed-energy {max_E}'.split()
    run(histogram(name, de, translation_scale=translation_scale)
        + f'--save-as {save}.cbor'.split()
        + f'--max-iter {max_iter} --sad-min-T {min_T}'.split()
        + max_E_args, check=True)


def run_wl(name, de, min_E, max_E, min_gamma=None, max_iter=max_iter_default, translation_scale=0.05):
    de = str(de)
    save = 'wl-'+name+'-'+de
    min_gamma_args = []
    if min_gamma is not None:
        min_gamma_args = f'--wl-min-gamma {min_gamma}'.split()
    run(histogram(name, de, translation_scale=translation_scale)
        + f'--save-as {save}.cbor'.split()
        + f'--max-iter {max_iter} --wl --min-allowed-energy {min_E} --max-allowed-energy {max_E}'.split()
        + min_gamma_args, check=True)


def run_inv_t_wl(name, de, min_E, max_E, max_iter=max_iter_default, translation_scale=0.05):
    de = str(de)
    save = 'itwl-'+name+'-'+de
    run(histogram(name, de, translation_scale=translation_scale)
        + f'--save-as {save}.cbor'.split()
        + f'--max-iter {max_iter} --inv-t-wl --min-allowed-energy {min_E} --max-allowed-energy {max_E}'.split(), check=True)


volumes = np.arange(1.0, 2.501, 0.5)
min_T = 0.1

systems = {
    'lj31-like': '--two-wells-N 90 --two-wells-h-2-to-h-1 1.1 --two-wells-r-2 0.2'.split(),
    'lj31-deeper': '--two-wells-N 90 --two-wells-h-2-to-h-1 1.2 --two-wells-r-2 0.2'.split(),
}

run_replicas(name='lj31-like', min_T=0.001, max_iter=1e12)
run_replicas(name='lj31-deeper')
