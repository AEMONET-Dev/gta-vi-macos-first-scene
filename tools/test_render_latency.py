import unittest

from render_latency import analyze, planned

WORLD = "INFO victory_gpu::capture: the world's first frame frame=100"
F109 = "INFO render_latency: frame delivered frame=109 t=10000 drawn=100 dispatched=20"
F110 = "INFO render_latency: frame delivered frame=110 t=11000 drawn=125 dispatched=23"
F111 = "INFO render_latency: frame delivered frame=111 t=12000 drawn=149 dispatched=25"
S1 = ("INFO render_latency: submission timeline req=9800 acquired=10000 descriptors=10100 "
      "heaps=10200 prepared=10600 gpu_end=10750 writeback=10800 done=10900")
S2 = ("INFO render_latency: submission timeline req=10500 acquired=10950 descriptors=11000 "
      "heaps=11020 prepared=11100 gpu_end=11200 writeback=11250 done=11300")
B1 = "INFO render_latency: buffer batch begin=10250 end=10500 schedule=false workers=4 jobs=20 planned=300 copied=290"
B2 = "INFO render_latency: buffer batch begin=10400 end=10580 schedule=false workers=4 jobs=13 planned=20 copied=20"
H = ("INFO render_latency: 174MiB refresh frame=109 world=9 enabled=true attempted=true success=true "
     "stored=1024 requested=182452224 begin=10400 end=10500 elapsed_us=100 attempted_us=98")
SAVE = r"INFO victory_gpu::capture: frame saved path=C:\out\frame-110.png"


class TestRenderLatency(unittest.TestCase):
    def test_partition_and_mode(self):
        r = analyze([WORLD, F109, F110, S2, F111, B2, B1, SAVE, S1], "abba")
        a, b = r["frames"]
        self.assertEqual(r["experiment"], "schedule")
        self.assertEqual([a["planned_start"], a["planned_end"]], ["off", "on"])
        self.assertEqual(a["mode"], "off")  # direct observation despite the policy transition
        self.assertEqual(b["mode"], "unknown")
        self.assertEqual(a["exclusive_ms"]["deferred_buffer_batch"], .33)
        self.assertEqual(a["exclusive_ms"]["deferred_other"], .07)
        self.assertEqual(a["exclusive_ms"]["queue_wait_only"], .05)
        self.assertEqual(a["nested_ms"]["queue_wait_union"], .45)
        self.assertEqual(a["nested_ms"]["queue_wait_while_active"], .40)
        self.assertEqual(a["exclusive_ms"]["outside_recorded_submission"], 0)
        self.assertAlmostEqual(sum(a["exclusive_ms"].values()), 1)
        self.assertEqual(b["exclusive_ms"]["outside_recorded_submission"], .70)
        self.assertEqual(a["dispatches"], 3)
        self.assertEqual(a["draws"], 25)
        self.assertEqual(a["planned_bytes_started"], 320)
        self.assertEqual(r["by_mode_capture"]["off"]["captured"]["n"], 1)

    def test_xmem_boundary_and_worker_time(self):
        long = H.replace("begin=10400 end=10500 elapsed_us=100","begin=10900 end=11100 elapsed_us=200")
        second = H.replace("begin=10400 end=10500", "begin=10950 end=11050")
        r = analyze([WORLD, F109, F110, F111, B1.replace("schedule=false", "xmem=false"), long, second], "baab")
        a, b = r["frames"]
        self.assertEqual(r["experiment"], "xmem")
        self.assertEqual(a["mode"], "mixed")
        self.assertEqual(a["hot_calls_started"], 2)
        self.assertEqual(b["hot_calls_started"], 0)
        self.assertEqual(b["mode"], "on")
        self.assertEqual(b["nested_ms"]["hot_copy_union"], .1)
        self.assertEqual(b["hot_worker_ms"], .15)
        self.assertEqual(r["hot_calls_by_mode"]["on"]["successes"], 2)
        self.assertEqual(r["hot_calls_by_mode"]["on"]["stored_bytes"], 2048)
        self.assertEqual(a["exclusive_ms"]["outside_recorded_submission"], 1)

    def test_missing_and_invalid(self):
        broken = F110.replace("frame=110", "frame=106")
        r = analyze([WORLD, broken.replace("t=11000", "t=9000"), F109, F110], "abba", 10)
        self.assertEqual(len(r["frames"]), 1)
        self.assertEqual(len(r["skipped_anchor_intervals"]), 1)
        self.assertEqual(r["selected_frames"]["n"], 0)
        with self.assertRaisesRegex(ValueError, "out of order"):
            analyze([WORLD, F109, F110, S1.replace("gpu_end=10750", "gpu_end=9600")])
        with self.assertRaisesRegex(ValueError, "need a world marker"):
            analyze([F109, F110])

    def test_fence_substeps_remain_nested(self):
        fence = ("INFO render_latency: fence timeline sequence=12 group=0 lists=3 started=10600 submitted=10620 "
                 "signaled=10640 armed=10650 returned=10730 waited=true")
        failed = ("INFO render_latency: fence timeline sequence=13 group=0 lists=1 started=10730 submitted=10736 "
                  "signaled=10740 armed=10740 returned=10750 waited=false")
        a = analyze([WORLD, F109, F110, S1, fence, failed])["frames"][0]
        self.assertEqual(a["exclusive_ms"]["submit_fence"], .15)
        self.assertEqual(a["nested_ms"]["fence_execute_call"], .026)
        self.assertEqual(a["nested_ms"]["fence_arm_event_call"], .01)
        self.assertEqual(a["nested_ms"]["fence_wait_return"], .08)
        self.assertEqual(a["nested_ms"]["fence_without_wait_return"], .01)
        self.assertEqual(a["fence_calls_without_wait_started"], 1)
        self.assertAlmostEqual(sum(a["exclusive_ms"].values()), 1)

    def test_snapshot_clipping_empty_marker_and_other_feature(self):
        snap = "INFO render_latency: snapshot timeline merge=false declarations=2 begin=10900 end=11100 elapsed_us=200"
        switched = "INFO render_latency: snapshot timeline merge=true declarations=0 begin=11000 end=11000 elapsed_us=0"
        zero = switched.replace("begin=11000 end=11000", "begin=12001 end=12001")
        f112 = F111.replace("frame=111 t=12000", "frame=112 t=13000")
        f113 = f112.replace("frame=112 t=13000", "frame=113 t=14000")
        lines = [WORLD, F109, B1.replace("schedule=false", "xmem=false"), S1, F110,
                 snap, switched, zero, F111, f112, f113]
        r = analyze(lines, "abba", 0, "snapshots")
        a, b, c, d = r["frames"]
        self.assertEqual(r["experiment"], "snapshots")
        self.assertEqual([x["mode"] for x in (a,b,c,d)], ["off","mixed","on","unknown"])
        self.assertEqual([x["snapshot_applied_mode"] for x in (a,b,c,d)], ["off","off","no_declarations","no_declarations"])
        self.assertEqual(b["nested_ms"]["snapshot_union"], .1)
        self.assertEqual(a["snapshot_declarations_started"], 2)
        self.assertEqual(b["snapshot_declarations_started"], 0)
        self.assertEqual(r["by_applied_snapshot_mode"]["no_declarations"]["n"], 2)
        self.assertEqual(r["v2_event_counts"]["snapshots"], 3)

    def test_material_dump_after_delivered_png(self):
        mat = r"INFO victory_gpu::capture: frame saved path=C:\out\material-110-002-base.png"
        r = analyze([WORLD, F109, SAVE, F110, mat, F111, B1, S1, S2], "abba")
        a, b = r["frames"]
        self.assertEqual(a["png_saves_in_interval"], {"frame": 1})
        self.assertEqual(b["png_saves_in_interval"], {"material": 1})
        self.assertFalse(b["end_frame_captured"])
        self.assertFalse(r["by_mode_without_png_saves"]["unknown"]["n"])

    def test_abba_source_equivalence(self):
        for label, values in (("abba", ["off","off","on","on","on","on","off","off"]),
                              ("baab", ["on","on","off","off","off","off","on","on"])):
            self.assertEqual([planned(label, 8+i) for i in range(8)], values)
            self.assertEqual(planned(label, 7), "off")


if __name__ == "__main__":
    unittest.main()
