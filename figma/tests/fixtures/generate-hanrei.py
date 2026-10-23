"""Regenerate the small Hanrei UI fixture and SQLite FTS5 database (python3)."""
import json
import re
import sqlite3
from pathlib import Path

root = Path(__file__).parent / 'public'
cases = [dict(
    case_id=92002, case_number='令和6(受)2', case_name='検索テストの損害賠償請求事件',
    judged_on='令和8年8月28日', court='最高裁判所第二小法廷', judgment_type='判決', result='棄却',
    reporter='民集第80巻', lower_court='東京高等裁判所', lower_case_number='令和5(ネ)1', lower_judged_on='令和6年3月1日',
    holding='使用者責任の成否', summary='被用者の加害行為についての判断。', referenced_laws='民法715条1項',
    body_citations=['民法７１５条'], body_text='主文\n本件上告を棄却する。\n' + '当事者の主張と証拠を検討する。' * 30 + '\n電子計算機の利用と立証責任について、次のとおり判断する。\n<script>alert("本文")</script>',
    detail_url='https://www.courts.go.jp/hanrei/92002/detail2/index.html',
    pdf_url='https://www.courts.go.jp/assets/hanrei/hanrei-pdf-92002.pdf',
), dict(
    case_id=92001, case_number='令和5(行ヒ)1', case_name='旧データの行政事件',
    judged_on='令和7年1月15日', court='東京地方裁判所', judgment_type='判決', result='認容',
    reporter='', lower_court='', lower_case_number='', lower_judged_on='',
    holding='行政処分の適法性', summary='', referenced_laws='行政事件訴訟法3条', body_citations=[],
    detail_url='https://www.courts.go.jp/hanrei/92001/detail2/index.html', pdf_url=None,
)]
(root / 'hanrei').mkdir(exist_ok=True)
keys = ['case_id', 'case_number', 'case_name', 'judged_on', 'court', 'result', 'holding']
for case in cases:
    (root / 'hanrei' / f'{case["case_id"]}.json').write_text(json.dumps(case, ensure_ascii=False, indent=2) + '\n')
(root / 'hanrei' / 'index.json').write_text(json.dumps(dict(source='https://www.courts.go.jp/hanrei/search1/index.html', count=len(cases), cases=[{k: c[k] for k in keys} for c in cases]), ensure_ascii=False, indent=2) + '\n')

# Use the Rust schema itself so the browser test exercises the current column order.
src = (Path(__file__).resolve().parents[3] / 'crates/search-index/src/lib.rs').read_text()
schema = re.search(r'CREATE VIRTUAL TABLE hanrei_fts USING fts5\(.*?\);', src, re.S).group(0)
path = root / 'hanrei-search.db'
path.unlink(missing_ok=True)
conn = sqlite3.connect(path)
conn.execute(schema)
conn.execute('CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)')
conn.execute("INSERT INTO meta VALUES ('hanrei_count', '2')")
# Keep the fixture larger than one httpvfs range chunk.
conn.execute('CREATE TABLE padding (data BLOB)')
conn.execute('INSERT INTO padding VALUES (zeroblob(65536))')

def tokens(text):
    out = []
    for part in re.findall(r'[\u3040-\u30ff\u3400-\u9fff]+|[A-Za-z0-9]+', text):
        out.extend([part[i:i+2] for i in range(len(part)-1)] if '\u3040' <= part[0] <= '\u9fff' else [part.lower()])
    return ' '.join(out)

for c in cases:
    values = [c[k] for k in keys] + [c['detail_url'], c['pdf_url'] or ''] + [tokens(c['case_name'] + ' ' + c['case_number']), tokens(c['holding']), tokens(c['summary']), tokens(c['referenced_laws']), tokens(' '.join(c['body_citations'])), tokens(c.get('body_text', ''))]
    conn.execute('INSERT INTO hanrei_fts VALUES (' + ','.join('?' for _ in values) + ')', values)
conn.commit()
conn.close()
