import importlib.util
import json
import os
import shutil
from pathlib import Path
import subprocess
import sys
import unittest
from unittest import mock
import tempfile

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('bench', ROOT / 'scripts/bench.py')
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)


class SelectionTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        (ROOT / '.work/tmp').mkdir(parents=True, exist_ok=True)

    def call(self, *args, env=None):
        return subprocess.run([sys.executable, str(ROOT / 'scripts/bench.py'), *args], cwd=ROOT, capture_output=True, text=True, env=env)

    def test_tiers_and_distinct_filters(self):
        edit = self.call('run', '--tier', 'edit', '--dry-run')
        self.assertEqual(edit.returncode, 0)
        self.assertIn('selected driver-test:', edit.stdout)
        self.assertIn('excluded corpus: above tier ceiling', edit.stdout)
        area = self.call('run', '--area', 'extraction', '--dry-run')
        self.assertEqual(area.returncode, 0)
        self.assertIn('selected corpus:', area.stdout)
        self.assertIn('excluded driver-test: area filter', area.stdout)
        self.assertNotEqual(self.call('run', '--name', 'extraction', '--dry-run').returncode, 0)
        self.assertNotEqual(self.call('run', '--skip', 'missing', '--dry-run').returncode, 0)
        self.assertNotEqual(self.call('run', '--tier', 'edit', '--skip', 'driver-test', '--dry-run').returncode, 0)
        self.assertNotEqual(self.call('run', '--tier', 'full').returncode, 0)

    def test_failed_workload_fails_run(self):
        with tempfile.TemporaryDirectory(dir=ROOT / '.work/tmp') as directory:
            with mock.patch.object(bench, 'ROOT', Path(directory)), \
                 mock.patch.object(bench, 'WORKLOADS', {'failure': ('edit', 'test', [sys.executable, '-c', 'raise SystemExit(7)'], 'failure')}), \
                 mock.patch.object(bench, 'preflight', return_value=[]), \
                 mock.patch.object(bench.subprocess, 'check_output', return_value='test'), \
                 mock.patch.object(sys, 'argv', ['bench.py', 'run', '--tier', 'edit']):
                self.assertEqual(bench.main(), 1)
                summary = next((Path(directory) / '.work/bench').glob('*/summary.txt')).read_text()
                self.assertIn('failure: exit 7', summary)

    def test_requires_both_completed_corpus_measurements(self):
        with tempfile.TemporaryDirectory(dir=ROOT / '.work/tmp') as directory:
            report = Path(directory) / 'corpus.json'
            html = {'name': 'fixture_bench::upstream_fixtures_html', 'sample_count': 5, 'iter_count': 5, 'median_ns': 100}
            markdown = dict(html, name='fixture_bench::upstream_fixtures_markdown')
            for measurements, expected in [
                ([], False),
                ([html], False),
                ([html, html], False),
                ([html, dict(markdown, iter_count=0)], False),
                ([html, dict(markdown, median_ns=float('nan'))], False),
                ([html, markdown], True),
            ]:
                with self.subTest(measurements=measurements):
                    report.write_text(json.dumps({'schema': 1, 'benchmarks': measurements}))
                    self.assertEqual(bench.corpus_measurements_complete(report), expected)

    def test_dry_run_and_failed_preflight_do_not_create_runs(self):
        with tempfile.TemporaryDirectory(dir=ROOT / '.work/tmp') as directory:
            root = Path(directory)
            (root / 'scripts').mkdir()
            shutil.copy(ROOT / 'scripts/bench.py', root / 'scripts/bench.py')
            command = [sys.executable, str(root / 'scripts/bench.py'), 'run', '--name', 'corpus']
            self.assertEqual(subprocess.run(command + ['--dry-run'], capture_output=True).returncode, 0)
            tools = root / 'bin'
            tools.mkdir()
            (tools / 'python3').symlink_to(sys.executable)
            env = dict(os.environ, PATH=str(tools))
            result = subprocess.run(command, cwd=root, capture_output=True, text=True, env=env)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('missing cargo', result.stderr)
            self.assertFalse((root / '.work/bench').exists())



if __name__ == '__main__': unittest.main()
