"""Independent source slices and original nested cells; no Rust/parser."""
from pathlib import Path
import json,sys
sys.dont_write_bytecode=True
from gold_logic import construct,H
P=Path(__file__).parent
proof=json.loads((P/'source-proof192.json').read_text())
baseline=json.loads((P.parent/'native-role-prefix/baseline/226-three.json').read_text())
assert H(baseline)==proof['window_baseline_sha256']
expected=construct(baseline,2,proof['owner_window'])
stored=json.loads((P/'expected/226-three.json').read_text())
assert expected==stored and H(expected)==proof['window_expected_sha256']
index=json.loads((P.parent/'native-role-prefix/index.json').read_text())
case=next(c for c in index['cases']if c['name']=='226-three')
# native263 cuts this gold at a katakana heading; katakana-headings263 replays that cut.
assert case['current_expected_path']=='tests/data/katakana-headings263/226-three.json'
assert case['expected_sha256']==proof['window_baseline_sha256'] # historical191 abstention retained
print('complete source-owned Word slices and cells exact; historical191 gold retained')
