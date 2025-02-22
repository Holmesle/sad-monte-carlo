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


def run_replicas(name, max_iter=max_iter_default, min_T=0.001, max_independent_samples=None, extraname='', extraflags=''):
    save = f'z-{extraname}{name}'
    samples = []
    if max_independent_samples is not None:
        samples = ['--max-independent-samples', str(max_independent_samples)]
    rq(name=save,
       cmd=['../target/release/replicas']+systems[name]+movie_args
        + f'--save-time 0.5 --save-as {save}.cbor'.split()
        + extraflags.split()
        + f'--max-iter {max_iter} --min-T {min_T}'.split()
        + samples,
       cpus='all')


# def binning_histogram(name, de, translation_scale):
#     return f'../target/release/binning --save-time 0.5 --histogram-bin {de} --translation-scale {translation_scale}'.split()+movie_args+systems[name]

def histogram(name, de, translation_scale):
    return f'../target/release/histogram --save-time 0.5 --energy-bin {de} --translation-scale {translation_scale}'.split()+movie_args+systems[name]

def run_sad(name, de, max_iter=max_iter_default, min_T=0.001, max_E=None, translation_scale=0.05):
    de = str(de)
    save = 'sad-'+name+'-'+de
    max_E_args = []
    if max_E is not None:
        max_E_args = f'--max-allowed-energy {max_E}'.split()
    rq(name=save,
       cmd=histogram(name, de, translation_scale=translation_scale)
        + f'--save-as {save}.cbor'.split()
        + f'--max-iter {max_iter} --sad-min-T {min_T}'.split()
        + max_E_args,
       cpus='1')


def run_wl(name, de, min_E, max_E, min_gamma=None, max_iter=max_iter_default, translation_scale=0.05):
    de = str(de)
    save = 'wl-'+name+'-'+de
    min_gamma_args = []
    if min_gamma is not None:
        min_gamma_args = f'--wl-min-gamma {min_gamma}'.split()
    rq(name=save,
       cmd=histogram(name, de, translation_scale=translation_scale)
        + f'--save-as {save}.cbor'.split()
        + f'--max-iter {max_iter} --wl --min-allowed-energy {min_E} --max-allowed-energy {max_E}'.split()
        + min_gamma_args,
       cpus='1')


def run_inv_t_wl(name, de, min_E, max_E, max_iter=max_iter_default, translation_scale=0.05):
    de = str(de)
    save = 'itwl-'+name+'-'+de
    rq(name=save,
       cmd=histogram(name, de, translation_scale=translation_scale)
        + f'--save-as {save}.cbor'.split()
        + f'--max-iter {max_iter} --inv-t-wl --min-allowed-energy {min_E} --max-allowed-energy {max_E}'.split(),
       cpus='1')

volumes = np.arange(2.6, 0.95, -0.05)
min_T = 0.1

movie_args = '--movie-time 10^(1/2)'.split()

systems = {}
for v in volumes:
    d = 1.0/v
    name = f'wca-32-%.2f' % v
    systems[name] = f'--wca-reduced-density {d} --wca-N 32 --independent-systems-before-new-bin 16'.split()
    run_replicas(name=name, min_T = min_T, max_independent_samples=1000)

movie_args = '--movie-time 10^(1/8)'.split()

for v in volumes:
    d = 1.0/v
    name = f'wca-108-%.2f' % v
    systems[name] = f'--wca-reduced-density {d} --wca-N 108 --independent-systems-before-new-bin 16'.split()
    run_replicas(name=name, min_T = min_T, max_independent_samples=1000)

# for v in volumes:
#     d = 1.0/v
#     name = f'wca-256-{v}'
#     systems[name] = f'--wca-reduced-density {d} --wca-N 256 --independent-systems-before-new-bin 8'.split()
#     run_replicas(name=name, min_T = min_T, max_iter=1e11)

# for v in volumes:
#     d = 1.0/v
#     name = f'wca-256-{v}-i16'
#     systems[name] = f'--wca-reduced-density {d} --wca-N 256 --independent-systems-before-new-bin 16'.split()
#     run_replicas(name=name, min_T = min_T, max_iter=1e11)

# for v in volumes:
#     d = 1.0/v
#     name = f'wca-500-{v}-i16'
#     systems[name] = f'--wca-reduced-density {d} --wca-N 500 --independent-systems-before-new-bin 16'.split()
#     run_replicas(name=name, min_T = min_T, max_iter=1e12)

