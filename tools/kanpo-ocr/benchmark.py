"""Sequential end-to-end OCR comparison; outputs are always kept in a new directory."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import re
import statistics
import subprocess
import time
import unicodedata


def normalize(text):
    return ''.join(c for c in unicodedata.normalize('NFKC', text) if not c.isspace())


def distance(a, b):
    previous = list(range(len(b) + 1))
    for i, x in enumerate(a, 1):
        row = [i]
        for j, y in enumerate(b, 1):
            row.append(min(row[-1] + 1, previous[j] + 1, previous[j - 1] + (x != y)))
        previous = row
    return previous[-1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', required=True, type=Path)
    parser.add_argument('--ndlocr-bin', required=True)
    parser.add_argument('--ndlocr-receipt')
    parser.add_argument('--ndlocr-models', required=True)
    parser.add_argument('--tesseract-model', required=True)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--engines', nargs='+', choices=['ndlocr', 'tesseract'], default=['ndlocr', 'tesseract'])
    parser.add_argument('--det-conf-threshold', type=float, default=0.25)
    parser.add_argument('--min-line-confidence', type=float, default=0.3)
    args = parser.parse_args()
    if args.repeats < 2:
        parser.error('at least two repeats required')
    args.output.mkdir()
    manifest = json.loads(args.manifest.read_text())
    cli = Path(__file__).resolve().with_name('cli.cjs')
    report = {'platform': platform.platform(), 'manifest': manifest, 'repeats': args.repeats,
              'timing': 'new process each run; includes model snapshots/loading and output; OS caches not flushed; no persistent session',
              'normalization': 'NFKC + remove whitespace (evaluation only)', 'results': []}
    for sample in manifest['samples']:
        image = Path(sample['image']).resolve()
        assert hashlib.sha256(image.read_bytes()).hexdigest() == sample['sha256']
        for repeat in range(args.repeats):
            # Alternate first engine to reduce ordering bias. Never run concurrently.
            for engine in (args.engines if repeat % 2 == 0 else list(reversed(args.engines))):
                output = args.output / f"{sample['id']}-{engine}-{repeat}"
                command = ['node', str(cli), '--engine', engine,
                           '--model', args.ndlocr_models if engine == 'ndlocr' else args.tesseract_model,
                           '--ndlocr-bin', args.ndlocr_bin, '--output', str(output.resolve()),
                           '--width-points', str(sample['width_points']), '--height-points', str(sample['height_points']),
                           '--dpi', str(sample['dpi']), str(image)]
                if engine == 'ndlocr' and args.ndlocr_receipt:
                    command.extend(['--ndlocr-receipt', args.ndlocr_receipt])
                if engine == 'ndlocr':
                    command.extend(['--det-conf-threshold', str(args.det_conf_threshold), '--min-line-confidence', str(args.min_line_confidence)])
                measured = ['/usr/bin/time', '-l', *command] if platform.system() == 'Darwin' else command
                start = time.perf_counter()
                result = subprocess.run(measured, capture_output=True, text=True)
                seconds = time.perf_counter() - start
                (args.output / f"{output.name}.stderr").write_text(result.stderr)
                row = {'sample': sample['id'], 'engine': engine, 'repeat': repeat,
                       'seconds': seconds, 'exit': result.returncode, 'command': command}
                match = re.search(r'(\d+)\s+maximum resident set size', result.stderr)
                if match:
                    row['peak_rss_bytes_time_wrapper'] = int(match[1])
                if result.returncode != 0:
                    report['results'].append(row)
                    (args.output / 'report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
                    raise RuntimeError(f"{output.name} failed: {result.stderr[:2000]}")
                prediction = (output / 'recognized.txt').read_text()
                row['prediction_sha256'] = hashlib.sha256(prediction.encode()).hexdigest()
                if 'truth' in sample:
                    truth = normalize(sample['truth'])
                    row.update(characters=len(truth), edits=distance(truth, normalize(prediction)))
                    row['cer'] = row['edits'] / len(truth)
                report['results'].append(row)
                (args.output / 'report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
                print(json.dumps({k: v for k, v in row.items() if k != 'command'}), flush=True)
    report['summary'] = []
    for sample in manifest['samples']:
        for engine in args.engines:
            rows = [r for r in report['results'] if r['sample'] == sample['id'] and r['engine'] == engine]
            report['summary'].append({'sample': sample['id'], 'engine': engine,
                                      'first_seconds': rows[0]['seconds'],
                                      'median_seconds': statistics.median(r['seconds'] for r in rows),
                                      'repeat_median_seconds': statistics.median(r['seconds'] for r in rows[1:]),
                                      'deterministic_text': len({r['prediction_sha256'] for r in rows}) == 1,
                                      **{k: rows[0][k] for k in ['characters', 'edits', 'cer'] if k in rows[0]}})
    (args.output / 'report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')


if __name__ == '__main__':
    main()
