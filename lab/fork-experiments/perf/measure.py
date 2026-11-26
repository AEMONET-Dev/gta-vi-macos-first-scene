from pathlib import Path
import json, re, statistics, subprocess, sys
here=Path(__file__).resolve().parent
def run(args, path):
    with path.open('w') as log:
        subprocess.run([sys.executable,here/'run.py',*args,'--fxc'],stdout=log,stderr=subprocess.STDOUT,check=True)
schedules=['00 01 11 10','01 10 00 11','10 11 01 00','11 00 10 01']
data={k:[] for k in ('00','01','10','11')}
for block,sequence in enumerate(schedules):
    for pair in sequence.split():
        outfile=here/f'batch-{block}-{pair}.log'
        run(['mac_captured_indirect','--scoped',pair[0],'--merge',pair[1]],outfile)
        rows=[]
        for line in outfile.read_text().splitlines():
            if 'BENCH ' in line:
                values={k:float(v) for k,v in re.findall(r'(\w+)=([.\d]+)',line)}
                rows.append(values)
        assert len(rows)==10 and all(row['verified']==100 for row in rows)
        data[pair].append(rows)
        print(f'{block} {pair} {statistics.median(r["wall_ms"] for r in rows[2:]):.4f} ms',flush=True)
def stats(batches):
    rows=[r for batch in batches for r in batch[2:]]
    ms=[r['wall_ms'] for r in rows]
    return dict(rounds=len(rows),check_dispatches_including_warmups=sum(len(b) for b in batches)*100,
                median_batch_ms=statistics.median(ms),p95_batch_ms=sorted(ms)[-2],
                median_by_block_ms=[statistics.median(r['wall_ms'] for r in b[2:]) for b in batches],
                global_barriers_per_100=int(rows[0]['pre_global']+rows[0]['post_global']),
                scoped_barriers_per_100=int(rows[0]['pre_scoped']),gpu_timestamps_nonzero=sum(r['gpu_ms']>0 for r in rows))
(here/'results.json').write_text(json.dumps(dict(scope='100 clamped ExecuteIndirect calls, production bound() / done(), counting shader, completed wall time, 32 timed batches per combination, warmup 2 per process',by_scoped_merge={p:stats(b) for p,b in data.items()},all_batches=data),indent=2)+'\n')
for name in ['warp_card_compare_stores_the_hash_it_made_when_told','warp_card_compare_inline_readback_and_overflow_fallback','warp_count_buffer_ignores_garbage_tail_and_preserves_source','warp_window_leaves_unfilled_slots_empty_and_uncharged','warp_budget_saturates_without_wraparound_and_preserves_draw_fields','warp_scoped_clamp_orders_preceding_uav_write']:
    try:
        run([name],here/(name+'.log'))
    except subprocess.CalledProcessError as e:
        print(f'upstream test failed: {name}: {e}',flush=True)
