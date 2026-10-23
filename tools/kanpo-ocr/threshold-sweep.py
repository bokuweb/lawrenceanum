"""Tune only NDLOCR detection/line confidence thresholds on a development manifest."""
import argparse
import hashlib
import itertools
import json
from pathlib import Path
import subprocess
import time

from benchmark import normalize, distance


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', required=True, type=Path)
    parser.add_argument('--ndlocr-bin', required=True, type=Path)
    parser.add_argument('--models', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--det', nargs='+', type=float, default=[0.25, 0.5, 0.75, 0.9, 0.95])
    parser.add_argument('--line', nargs='+', type=float, default=[0.3, 0.7, 0.9])
    args = parser.parse_args()
    if any(not 0 <= v <= 1 for v in args.det + args.line):
        parser.error('thresholds must be between 0 and 1')
    args.output.mkdir()
    manifest = json.loads(args.manifest.read_text())
    samples = [s for s in manifest['samples'] if 'truth' in s]
    if not samples:
        parser.error('at least one truth sample required')
    for sample in samples:
        assert hashlib.sha256(Path(sample['image']).read_bytes()).hexdigest() == sample['sha256']
    flags = json.loads((Path(__file__).with_name('audits') / 'engine-comparison-20261023.json').read_text())['assets']['ndlocr']['models']
    model_flags = ['--det-model', '--model', '--model30', '--model50', '--charset']
    assets = []
    for flag, asset in zip(model_flags, flags):
        model = (args.models / Path(asset['path']).name).resolve()
        assert hashlib.sha256(model.read_bytes()).hexdigest() == asset['sha256']
        assets.extend([flag, str(model)])
    report = {'scope': 'threshold-only development tuning, not held-out accuracy',
              'manifest': manifest, 'det': args.det, 'line': args.line, 'results': []}
    for det, line in itertools.product(args.det, args.line):
        group = {'det': det, 'line': line, 'samples': [], 'edits': 0, 'characters': 0}
        for sample in samples:
            output = args.output / f"det{det}-line{line}-{sample['id']}.txt"
            command = [str(args.ndlocr_bin.resolve()), 'recognize-page', '--image', str(Path(sample['image']).resolve()),
                       *assets, '--structure-rules=false', '--det-conf-threshold', str(det),
                       '--min-line-confidence', str(line), '--output-txt', str(output.resolve())]
            start = time.perf_counter()
            result = subprocess.run(command, capture_output=True, text=True)
            output.with_suffix('.stderr').write_text(result.stderr)
            if result.returncode:
                raise RuntimeError(f'{command}: {result.stderr}')
            text = output.read_text()
            gold = normalize(sample['truth'])
            edits = distance(gold, normalize(text))
            group['samples'].append({'id': sample['id'], 'edits': edits, 'characters': len(gold),
                                     'cer': edits / len(gold), 'prediction': text, 'command': command,
                                     'seconds': time.perf_counter() - start})
            group['edits'] += edits
            group['characters'] += len(gold)
        group['cer'] = group['edits'] / group['characters']
        report['results'].append(group)
        report['best'] = min(report['results'], key=lambda row: row['edits'])
        (args.output / 'report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
        print(json.dumps({k: v for k, v in group.items() if k != 'samples'}), flush=True)


if __name__ == '__main__':
    main()
