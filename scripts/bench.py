#!/usr/bin/env python3
import argparse
import datetime
from pathlib import Path
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parent.parent
TIERS = ('edit', 'change', 'scenario', 'full')
WORKLOADS = {
    'driver-test': ('edit', 'driver', ['python3', '-m', 'unittest', 'discover', '-s', 'tests', '-p', 'bench_driver_test.py'], 'selection and preflight contracts; seconds'),
    'corpus': ('change', 'extraction', ['cargo', 'bench', '--locked', '-p', 'defuddle', '--bench', 'fixture_bench', '--', '--format', 'json', '--threads', '1', '--sample-count', '5', '--sample-size', '1'], 'HTML and Markdown corpus; estimated minutes including build'),
    'corpus-legacy': ('full', 'extraction', ['cargo', 'bench', '-p', 'defuddle', '--bench', 'fixture_bench'], 'original make bench; harness defaults, estimated minutes'),
}


def selection(args):
    for label, values, valid in [('name', args.name, WORKLOADS), ('area', args.area, {v[1] for v in WORKLOADS.values()}), ('skip', args.skip, WORKLOADS)]:
        unknown = set(values) - set(valid)
        if unknown:
            raise ValueError('unknown %s: %s' % (label, ', '.join(sorted(unknown))))
    selected, excluded = [], []
    for name, (tier, area, command, purpose) in WORKLOADS.items():
        reason = None
        if TIERS.index(tier) > TIERS.index(args.tier): reason = 'above tier ceiling'
        elif args.name and name not in args.name: reason = 'name filter'
        elif args.area and area not in args.area: reason = 'area filter'
        elif name in args.skip: reason = 'skipped by request'
        (excluded if reason else selected).append((name, reason))
    if not selected: raise ValueError('selection matches no workloads')
    return selected, excluded


def preflight(selected):
    errors = []
    names = ', '.join(name for name, _ in selected)
    for tool in {'python3'} | ({'cargo', 'git', 'rustup'} if any(name.startswith('corpus') for name, _ in selected) else set()):
        if not shutil.which(tool): errors.append('missing %s; install it or restore PATH (needed by %s)' % (tool, names))
    if any(name.startswith('corpus') for name, _ in selected):
        fixtures = list((ROOT / 'tests/upstream/defuddle/fixtures').glob('*.html'))
        if len(fixtures) != 168: errors.append('expected 168 fixtures, found %d; restore vendored snapshot (needed by corpus)' % len(fixtures))
        if not (ROOT / 'Cargo.lock').is_file(): errors.append('missing Cargo.lock; restore it (needed by corpus)')
        if shutil.disk_usage(ROOT).free < 1024**3: errors.append('less than 1 GiB free; free disk space (needed by corpus)')
        if shutil.which('git'):
            dirty = subprocess.run(['git', 'status', '--porcelain', '--', 'tests/upstream/defuddle', 'tests/upstream/SNAPSHOT.md'], cwd=ROOT, capture_output=True, text=True)
            if dirty.returncode or dirty.stdout: errors.append('fixture snapshot modified or untracked; restore it (needed by corpus)')
        if shutil.which('rustup'):
            active = subprocess.run(['rustup', 'show', 'active-toolchain'], cwd=ROOT, capture_output=True, text=True)
            if active.returncode or not active.stdout.startswith('nightly-2026-11-09-'):
                errors.append('pinned nightly-2026-11-09 is not active; install it and remove overrides (needed by corpus)')
            else:
                print('Toolchain: ' + active.stdout.strip())
        print('Host readiness: not qualified; run perf-gate-ready before and after comparisons.')
        print('Cargo builds the benchmark from current sources; build and OS caches are uncontrolled.')
    return errors


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('action', choices=['run', 'check', 'list'])
    parser.add_argument('--tier', choices=TIERS, default='change')
    parser.add_argument('--name', action='append', default=[])
    parser.add_argument('--area', action='append', default=[])
    parser.add_argument('--skip', action='append', default=[])
    parser.add_argument('--dry-run', action='store_true')
    parser.add_argument('--allow-full', action='store_true')
    args = parser.parse_args()
    if args.action == 'list':
        for name, (tier, area, command, purpose) in WORKLOADS.items():
            print('%s: tier=%s area=%s; %s; command=%s' % (name, tier, area, purpose, ' '.join(command)))
        return 0
    try: selected, excluded = selection(args)
    except ValueError as error:
        print(error, file=sys.stderr); return 2
    for name, reason in excluded: print('excluded %s: %s' % (name, reason))
    if args.tier == 'full' and args.action == 'run' and not args.dry_run and not args.allow_full:
        print('Full is manual-only; explicitly pass --allow-full', file=sys.stderr); return 2
    for name, _ in selected: print('selected %s: %s' % (name, ' '.join(WORKLOADS[name][2])))
    if args.dry_run: return 0
    errors = preflight(selected)
    if errors:
        for error in errors: print('preflight: ' + error, file=sys.stderr)
        return 1
    if args.action == 'check': print('preflight passed'); return 0
    run_dir = ROOT / '.work/bench' / datetime.datetime.now().strftime('%Y-%m-%d_%H%M%S_%f')
    run_dir.mkdir(parents=True)
    lines = ['commit: ' + subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(), 'dirty state:\n' + subprocess.check_output(['git', 'status', '--short'], cwd=ROOT, text=True), 'timing validity: not readiness-qualified; completion is not a performance verdict', 'exclusions:']
    lines.extend('%s: %s' % item for item in excluded)
    failed = False
    for name, _ in selected:
        started = time.monotonic()
        with (run_dir / (name + '.log')).open('w') as log:
            result = subprocess.run(WORKLOADS[name][2], cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
        duration = time.monotonic() - started
        lines.append('%s: exit %d; %.2f s; log %s.log; command: %s' % (name, result.returncode, duration, name, ' '.join(WORKLOADS[name][2])))
        failed |= result.returncode != 0
    (run_dir / 'summary.txt').write_text('\n'.join(lines) + '\n')
    print('\n'.join(lines)); print('run directory: ' + str(run_dir))
    return int(failed)


if __name__ == '__main__': sys.exit(main())
