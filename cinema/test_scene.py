"""Offline tests of the shot language (no game, no Boon): a synthetic take and its track cache in a temp folder.

    python -I test_scene.py
"""
import json
import os
import shutil
import sys
import tempfile
import unittest

TMP = tempfile.mkdtemp(prefix="cinema_test_")
os.environ["CINEMA_WORK"] = TMP
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import scene as scn  # noqa: E402
import film  # noqa: E402

TAKE = os.path.join(TMP, "synthetic.dem")


def setUpModule():
    open(TAKE, "wb").write(b"not a demo: the track cache below stands in for Boon")
    # Lash runs along +x at 320 units a second from tick 1000; Sinclair stands at (500, 300); a cast at tick 1100
    lash = [[t, 0.0 + max(0, t - 1000) * 5.0, 0.0, 0.0, 0.0, 0.0, 780, True] for t in range(900, 1400)]
    sin = [[t, 500.0, 300.0, 0.0, 0.0, 180.0, 750, True] for t in range(900, 1400)]
    data = {"take": TAKE, "build": 1, "map": "new_player_basics", "total_ticks": 1400, "tick_rate": 64,
            "pawns": {"31:1": {"hero": "Lash", "hero_id": 31, "steam_id": 1, "samples": lash},
                      "60:2": {"hero": "Sinclair", "hero_id": 60, "steam_id": 2, "samples": sin}},
            "abilities": [[1100, "Lash", "citadel_ability_lash_down_strike"], [1300, "Sinclair", "ability_magician_magicbolt"]]}
    os.makedirs(os.path.join(TMP, "tracks"))
    json.dump(data, open(os.path.join(TMP, "tracks", "synthetic.json"), "w"))


TAKE2 = os.path.join(TMP, "city.dem")
SCENE_TAKES = """
[actors]
faz = { main = "Sinclair", city = "Sinclair#0" }
maid = "Lash"

[[shot]]
length = 0.5
tick = 1000
[shot.camera]
preset = "chase"
actor = "maid"

[[shot]]
take = "city"
length = 0.5
tick = "cast faz animalhex -10"
[shot.camera]
preset = "chase"
actor = "faz"
"""


def city_take():
    """A second synthetic take: Sinclair runs along +y from (1000, 0) at 320 units a second; a cast at tick 300."""
    if not os.path.exists(TAKE2):
        open(TAKE2, "wb").write(b"not a demo")
        sin = [[t, 1000.0, max(0, t - 200) * 5.0, 600.0, 0.0, 90.0, 750, True] for t in range(100, 600)]
        data = {"take": TAKE2, "build": 1, "map": "dl_midtown", "total_ticks": 600, "tick_rate": 64,
                "pawns": {"60:2": {"hero": "Sinclair", "hero_id": 60, "steam_id": 2, "samples": sin}},
                "abilities": [[300, "Sinclair", "ability_magician_animalhex"]]}
        json.dump(data, open(os.path.join(TMP, "tracks", "city.json"), "w"))


def tearDownModule():
    shutil.rmtree(TMP, ignore_errors=True)


def load(text, scene_extra=""):
    p = os.path.join(TMP, "s.toml")
    with open(p, "w", encoding="utf-8") as f:
        f.write(f'[scene]\nname = "t"\ntake = "{TAKE.replace(chr(92), "/")}"\nfps = 32\n' + scene_extra + text)
    return scn.load(p)


class ShotLanguage(unittest.TestCase):
    def test_real_time_is_two_ticks_a_frame(self):
        sc = load('[[shot]]\nlength = 1\ntick = 1000\n[shot.camera]\ntype = "fixed"\npos = [0, -300, 60]\nlook = "Lash"\n')
        whole, steps = sc.shots[0].ticks()
        self.assertEqual(len(whole), 32)
        self.assertEqual(set(steps[1:]), {2})
        self.assertEqual(sc.shots[0].mode(), "lockstep")

    def test_slow_below_a_tick_a_frame_goes_substep(self):
        sc = load('[[shot]]\nlength = 1\ntick = 1000\nspeed = 0.25\ncamera = "side_track"\n[shot.defaults]\n')
        self.assertEqual(sc.shots[0].mode(), "substep")

    def test_presets_aliases_and_defaults(self):
        sc = load('[actors]\nmaid = "Lash#0"\n[defaults]\nspeed = 0.5\n[defaults.camera]\nlook_height = 70\n'
                  '[[shot]]\nlength = 1\ntick = 1000\n[shot.camera]\npreset = "side_track"\nactor = "maid"\n'
                  '[[shot]]\nlength = 1\ntick = "after"\n[shot.camera]\npreset = "bullet"\nactor = "maid"\n')
        a, b = sc.shots
        self.assertEqual(a.speed, 0.5)
        self.assertEqual(a.cam["look_height"], 70, "defaults reach into the camera")
        self.assertEqual(a.cam["offset"], [30, -220, 55], "the preset fills the rest")
        self.assertEqual(b.cam["around"], "maid", "an orbit preset takes actor as its centre")
        f = sc.frames(a)[0]
        # side_track from the right of a runner heading +x: the camera is on -y and looks across to +y
        self.assertLess(f.pos[1], -150)
        self.assertAlmostEqual(f.yaw % 360, 90, delta=20)

    def test_tick_after_and_cast_anchors(self):
        sc = load('[[shot]]\nlength = 1\ntick = "cast down_strike -16"\n[shot.camera]\ntype = "fixed"\npos = [0, 0, 60]\nang = [0, 0]\n'
                  '[[shot]]\nlength = 0.5\ntick = "after"\n[shot.camera]\ntype = "fixed"\npos = [0, 0, 60]\nang = [0, 0]\n'
                  '[[shot]]\ntick = "cast Sinclair magicbolt"\nuntil = "cast Sinclair magicbolt +64"\n'
                  '[shot.camera]\ntype = "fixed"\npos = [0, 0, 60]\nang = [0, 0]\n')
        a, b, c = sc.shots
        self.assertEqual(a.tick, 1084)
        self.assertEqual(b.tick, a.ticks()[0][-1] + 2, "after: one step past the last frame of the shot before")
        self.assertEqual(c.tick, 1300)
        self.assertAlmostEqual(c.length, 1.0, "until: 64 ticks at speed 1 is a second")

    def test_cues_on_the_film_clock(self):
        sc = load('[[cue]]\nat = 0.2\ntext = "music in"\n'
                  '[[shot]]\nlength = 2\ntick = 1000\ncue = "title"\n[shot.camera]\ntype = "fixed"\npos = [0, 0, 60]\nang = [0, 0]\n'
                  '[[shot]]\nlength = 2\ntick = 1100\ncut = "dissolve 0.5"\ncues = [[1.0, "slam"]]\n'
                  '[shot.camera]\ntype = "fixed"\npos = [0, 0, 60]\nang = [0, 0]\n')
        self.assertEqual(sc.cues(), [[0.0, "title"], [0.2, "music in"], [2.5, "slam"]],
                         "a dissolve overlaps the shots: the second starts at 1.5 s")
        files = film.edit_files(sc, os.path.join(TMP, "edit"))
        edl = open(files[1]).read()
        self.assertIn("002  MASTER   V     D    016 00:00:01:16", edl)
        self.assertIn("slam", open(files[3]).read())

    def test_mistakes_say_where(self):
        cam = '[shot.camera]\ntype = "fixed"\npos = [0, 0, 60]\nang = [0, 0]\n'
        for text, want in [
            ('[[shot]]\nlength = 1\ntick = 1000\ncamera = "no_such"\n', "preset 'no_such' unknown"),
            ('[[shot]]\nlength = 1\ntick = 1000\n[shot.camera]\ntype = "track"\nactor = "Abrams"\n', "actor 'Abrams' is not in"),
            ('[[shot]]\nlength = 1\ntick = "cast nothing"\n' + cam, "no cast of 'nothing'"),
            ('[[shot]]\nlength = 1\ntick = "after"\n' + cam, '"after" on the first shot'),
            ('[[shot]]\ntick = 1000\n' + cam, "missing length"),
            ('[[shot]]\nname = "bad name"\nlength = 1\ntick = 1000\n' + cam, "letters, digits and _ only"),
        ]:
            with self.assertRaises((scn.SceneError, scn.track.TrackError)) as e:
                sc = load(text)
                for s in sc.shots:
                    sc.frames(s)
            self.assertIn(want, str(e.exception), text)
            self.assertIn("shot 1", str(e.exception) + "shot 1" * ("actor" in want), "names the shot")

    def test_a_scene_across_takes(self):
        city_take()
        sc = load(SCENE_TAKES, f'[takes]\ncity = "{TAKE2.replace(chr(92), "/")}"\n')
        a, b = sc.shots
        self.assertEqual(b.take_key, "city")
        self.assertEqual(b.tick, 290, "a cast anchor reads the shot's own take")
        f = sc.frames(b)[0]
        self.assertAlmostEqual(f.pos[0], 1000, delta=5, msg="the chase sits behind the city take's Sinclair")
        self.assertLess(f.pos[1], sc.actor("faz", b).at(290)[1])
        self.assertIn("take city", sc.summary())
        with self.assertRaises(scn.SceneError):
            load('[[shot]]\ntake = "nowhere"\nlength = 1\ntick = 1000\ncamera = "chase"\n')

    def test_keys_segments_settle(self):
        """A pan on keys with path = "segments": held before its first key, eased per segment, exactly on the last key
        from its time on (no spline overshoot), turning one way only."""
        sc = load('[[shot]]\nlength = 2\ntick = 1100\nspeed = 0\n[shot.camera]\ntype = "keys"\npath = "segments"\n'
                  'keys = [{ t = 0.25, pos = [0, -300, 60], look = [0, 0, 60] },\n'
                  '        { t = 1.5, pos = [0, -300, 60], look = [300, 0, 60], ease = "out" }]\n')
        fr = sc.frames(sc.shots[0])
        yaws = [f.yaw for f in fr]
        self.assertAlmostEqual(yaws[0], 90, places=6)
        self.assertEqual(yaws[0], yaws[8], "held before the first key (0.25 s is frame 8)")
        last = scn.look_angles([0, -300, 60], [300, 0, 60])[1]
        self.assertTrue(all(abs(y - last) < 1e-9 for y in yaws[48:]), "settled on the last key from 1.5 s (frame 48)")
        steps = [yaws[k] - yaws[k + 1] for k in range(8, 48)]
        self.assertTrue(all(s >= -1e-9 for s in steps), "one way only: never past the subject and back")
        self.assertLess(steps[-1], steps[0] / 10, "ease out: arrives slowly")
        self.assertEqual(sc.check(), [], "a held position at speed 0 is fine")
        bad = load('[[shot]]\nlength = 1\ntick = 1100\n[shot.camera]\ntype = "keys"\npath = "bezier"\n'
                   'keys = [{ t = 0, pos = [0, 0, 60], ang = [0, 0] }, { t = 1, pos = [0, 0, 60], ang = [0, 90] }]\n')
        with self.assertRaises(scn.SceneError):
            bad.frames(bad.shots[0])

    def test_frozen_travel_is_warned(self):
        """In a paused demo spec_goto turns the spectator but does not move it (cut 5's s20): a frozen shot whose
        camera travels is warned about."""
        sc = load('[[shot]]\nlength = 1\ntick = 1100\nspeed = 0\n[shot.camera]\npreset = "hero_orbit"\nactor = "Lash"\n')
        self.assertTrue(any("travels while the demo holds still" in w for w in sc.check()))
        sc = load('[[shot]]\nlength = 1\ntick = 1100\n[shot.camera]\npreset = "hero_orbit"\nactor = "Lash"\n')
        self.assertFalse(any("travels while" in w for w in sc.check()), "a moving demo moves the camera")

    def test_orbit_heading_relative(self):
        sc = load('[[shot]]\nlength = 1\ntick = 1100\nspeed = 0\n[shot.camera]\npreset = "hero_orbit"\nactor = "Lash"\n'
                  'from = 0\nto = 0\nradius = 200\n')
        f = sc.frames(sc.shots[0])[0]
        lash = sc.actor("Lash").at(1100)
        self.assertAlmostEqual(f.pos[0] - lash[0], 200, delta=5, msg="0 degrees is in front of a runner heading +x")


class Encoding(unittest.TestCase):
    """Frames piped into FFV1 (FrameSink), then the edit (a dissolve, a digital zoom, supersampling 2:1)."""

    def test_jump_detector(self):
        # a bar sweeping right two pixels a frame; frame 12 jumps the bar far away and frame 13 is back on its path
        # (a cloth stretched for a frame), frames 30-31 jump together: both found, the smooth sweep is not
        w, h = 320, 180
        path = os.path.join(TMP, "jumps.mkv")
        sink = film.FrameSink(path, w, h, 32)
        for k in range(40):
            x0 = 20 + 2 * k if k not in (12, 30, 31) else 250
            row = bytearray(b"\x20\x20\x20\xff" * w)
            for x in range(x0, x0 + 40):
                row[4 * x:4 * x + 3] = b"\xf0\xf0\xf0"
            sink.write(bytes(row) * h)
        sink.close()
        found = [(k, span) for k, span, _ in film.jumps(path)]
        self.assertEqual(found, [(12, 1), (30, 2)])

    def test_sink_and_edit(self):
        import subprocess
        cam = '[shot.camera]\ntype = "fixed"\npos = [0, 0, 60]\nang = [0, 0]\n'
        # a cut, then a dissolve after it (a concat's timebase once broke the xfade that followed it)
        sc = load('[[shot]]\nname = "a"\nlength = 1\ntick = 1000\n' + cam +
                  '[[shot]]\nname = "c"\nlength = 1\ntick = 1050\n' + cam +
                  '[[shot]]\nname = "b"\nlength = 1\ntick = 1100\ncut = "dissolve 0.25"\nzoom = [1, 2]\n' + cam,
                  scene_extra='window = [320, 180]\noutput = [160, 90]\n')
        for shot in sc.shots:
            d = os.path.join(film.out_dir(sc), "shots", shot.name)
            os.makedirs(d, exist_ok=True)
            sink = film.FrameSink(os.path.join(d, "shot.mkv"), 320, 180, sc.fps)
            for k in range(shot.n):
                sink.write(bytes([k * 7 % 256, 40, 200 if shot.name == "b" else 20, 0]) * (320 * 180))
            sink.close()
        master, preview = film.encode(sc, keep_frames=True)
        probe = subprocess.run(["ffprobe", "-v", "error", "-count_frames", "-select_streams", "v:0", "-show_entries",
                                "stream=nb_read_frames,width,height", "-of", "csv=p=0", master],
                               capture_output=True, text=True).stdout.strip()
        w, h, n = (int(v) for v in probe.split(","))
        self.assertEqual((w, h), (160, 90))
        self.assertEqual(n, 96 - 8, "three 32-frame shots, the last two overlapping 8 frames in the dissolve")


class TrailerLanguage(unittest.TestCase):
    """The pieces the Maid Lash trailer added: a tempo map, a planned take, cards, the snip, grades, the mix."""
    MUSIC = '[music]\nbpm = 120\nmeter = 4\n'
    BLOCK = ('[blocking]\nlength = 10.0\ncasts = [[4.0, "maid", "ultimate"]]\n'
             '[blocking.actors.maid]\nhero = "Lash"\nkeys = [{ t = 0.0, pos = [0, 0, 0], yaw = 90 }, '
             '{ t = 4.0, pos = [400, 0, 0] }]\n')
    CAM = '[shot.camera]\ntype = "fixed"\npos = [0, 0, 60]\nang = [0, 0]\n'

    def test_musical_time_and_the_beat_grid(self):
        cam = self.CAM
        sc = load('[[shot]]\nname = "a"\nlength = "1 bar"\ntick = 1000\n' + cam +
                  '[[shot]]\nname = "b"\nlength = "1 bar 1 beat"\ntick = 1100\ncut = "dissolve 1 beat"\n' + cam +
                  '[[shot]]\nname = "c"\nlength = "3 beats"\ntick = 1200\n' + cam +
                  '[[cue]]\nat = "bar 2 beat 3"\ntext = "x"\n', scene_extra=self.MUSIC)
        self.assertEqual([round(s.length, 3) for s in sc.shots], [2.0, 2.5, 1.5])
        self.assertEqual([round(a, 3) for _, a, _ in sc.timeline()], [0.0, 1.5, 4.0])
        self.assertEqual(sc.cue_table[0][0], 3.0)
        self.assertEqual(sc.beat_check(), [])
        off = load('[[shot]]\nname = "a"\nlength = 1.3\ntick = 1000\n' + cam, scene_extra=self.MUSIC)
        self.assertTrue(any("off the beat grid" in w for w in off.beat_check()))

    def test_a_planned_take(self):
        cam = '[shot.camera]\ntype = "fixed"\npos = [0, -300, 60]\nlook = "maid"\n'
        p = os.path.join(TMP, "plan.toml")
        with open(p, "w", encoding="utf-8") as f:
            f.write('[scene]\nname = "plan"\ntake = "blocking"\nfps = 32\n' + self.BLOCK +
                    '[[shot]]\nname = "a"\nlength = 1\ntick = "cast maid ultimate"\n' + cam)
        sc = scn.load(p)
        self.assertEqual(sc.shots[0].tick, 256)
        self.assertAlmostEqual(sc.actor("maid").at(128)[0], 200.0, delta=25)
        with self.assertRaises(RuntimeError):
            film.render(sc)

    def test_card_snip_grade_and_mix(self):
        import subprocess
        import grade
        self.assertIn("blend", grade.fragment("parlour", "i", "o", "t"))
        self.assertNotIn("ld(", film.snip_expr())          # no registers: xfade evaluates on several threads
        tone = os.path.join(TMP, "tone.wav")
        subprocess.run(["ffmpeg", "-y", "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i",
                        "sine=f=440:d=0.3", tone], check=True)
        cam = self.CAM
        sc = load('[[shot]]\nname = "a"\nlength = "2 beats"\ntick = 1000\ngrade = "brawler"\n' + cam +
                  '[[shot]]\nname = "b"\nlength = "2 beats"\ntick = 1100\ncut = "snip 1 beat"\ngrade = "fever"\n' + cam +
                  '[[shot]]\nname = "card"\nlength = "2 beats"\ncard = { title = "N°7", sub = "a: b, c" }\n'
                  f'[[sfx]]\nat = "bar 1 beat 2"\nfile = "{tone.replace(chr(92), "/")}"\n',
                  scene_extra='window = [320, 180]\noutput = [160, 90]\n' + self.MUSIC)
        for shot in sc.shots[:2]:
            d = os.path.join(film.out_dir(sc), "shots", shot.name)
            os.makedirs(d, exist_ok=True)
            sink = film.FrameSink(os.path.join(d, "shot.mkv"), 320, 180, sc.fps)
            for k in range(shot.n):
                sink.write(bytes([k * 9 % 256, 90, 160, 0]) * (320 * 180))
            sink.close()
        master, _ = film.encode(sc, keep_frames=True)
        streams = subprocess.run(["ffprobe", "-v", "error", "-show_entries", "stream=codec_type", "-of", "csv=p=0",
                                  master], capture_output=True, text=True).stdout.split()
        self.assertEqual(sorted(streams), ["audio", "video"])

    def test_layers_split_screen(self):
        """Two layer sources as panels over a host: in from the edges, held, apart again (the split screen)."""
        import subprocess
        cam = self.CAM
        nl = chr(10)
        keys_top = '[[0, 0, -0.5], ["4 frames", 0, 0, "out"], ["1 beat", 0, 0], ["1 beat 4 frames", 0, -0.5, "in"]]'
        keys_bot = '[[0, 0, 1], ["4 frames", 0, 0.5, "out"], ["1 beat", 0, 0.5], ["1 beat 4 frames", 0, 1, "in"]]'
        host = nl.join(['[[shot]]', 'name = "host"', 'length = "2 beats"', 'tick = 1000',
                        f'layers = [{{ src = "top", crop = [0, 0.25, 1, 0.5], size = [1, 0.5], keys = {keys_top}, border = 2 }},',
                        f'          {{ src = "bot", crop = [0, 0.25, 1, 0.5], size = [1, 0.5], keys = {keys_bot} }}]', ''])
        top = nl.join(['[[shot]]', 'name = "top"', 'layer = true', 'length = "2 beats"', 'tick = 1100', ''])
        bot = nl.join(['[[shot]]', 'name = "bot"', 'layer = true', 'length = "2 beats"', 'tick = "after"', ''])
        sc = load(host + cam + top + cam + bot + cam, scene_extra='window = [320, 180]' + nl + 'output = [160, 90]' + nl + self.MUSIC)
        self.assertEqual([s.name for s in sc.shots], ["host"])
        self.assertEqual([s.name for s in sc.layer_shots], ["top", "bot"])
        self.assertEqual(sc.layer_shots[1].tick, sc.layer_shots[0].ticks()[0][-1] + 2)   # "after" in its own group
        self.assertEqual(round(sc.timeline()[-1][2], 3), 1.0)                            # layers take no screen time
        L = sc.shots[0].layers[0]
        self.assertEqual([round(k[0], 4) for k in L["keys"]], [0.0, 0.125, 0.5, 0.625])
        self.assertIn("lt(t,0.125000)", film.layer_expr(L["keys"], 2, 90))
        colours = {"host": (255, 0, 0), "top": (0, 0, 255), "bot": (0, 255, 0)}   # BGR0 frames
        for shot in sc.all_shots():
            d = os.path.join(film.out_dir(sc), "shots", shot.name)
            os.makedirs(d, exist_ok=True)
            sink = film.FrameSink(os.path.join(d, "shot.mkv"), 320, 180, sc.fps)
            for k in range(shot.n):
                sink.write(bytes(colours[shot.name] + (0,)) * (320 * 180))
            sink.close()
        master, _ = film.encode(sc, keep_frames=True)

        def px(t, y):
            raw = subprocess.run(["ffmpeg", "-v", "error", "-ss", f"{t}", "-i", master, "-frames:v", "1", "-f", "rawvideo",
                                  "-pix_fmt", "rgb24", "-"], capture_output=True).stdout
            o = (y * 160 + 80) * 3
            return tuple(raw[o:o + 3])
        h, t, b = px(0.0, 20), px(0.3, 20), px(0.3, 70)
        self.assertNotEqual(t, h)                     # the top panel covers the top half while held
        self.assertNotEqual(b, h)                     # the bottom panel the bottom half
        self.assertNotEqual(t, b)
        self.assertEqual(px(0.85, 20), h)             # apart again: the host shows through
        self.assertEqual(px(0.85, 70), h)

    def test_layers_cards(self):
        """The collage: a card (a polaroid) snaps on at its first key, scaled and turned, holds its one-frame source,
        and is thrown off the frame; nothing draws before its first key; an image file can be a card's source."""
        import subprocess
        from capture import write_png
        cam = self.CAM
        nl = chr(10)
        keys = '[["1 beat", 0.5, 0.5, "out", 1.6, -10], ["1 beat 4 frames", 0.5, 0.5, "out", 1.0, -6], ["3 beats", 0.5, 0.5, "linear", 1.0, -6], ["3 beats 6 frames", 1.6, -0.6, "in", 1.3, 40]]'
        host = nl.join(['[[shot]]', 'name = "host"', 'length = "1 bar"', 'tick = 1000',
                        f'layers = [{{ src = "print", card = "polaroid", caption = "the bell", hold = true, size = [0.4, 0.4], keys = {keys} }}]', ''])
        pr = nl.join(['[[shot]]', 'name = "print"', 'layer = true', 'length = "2 frames"', 'speed = 0', 'tick = 1100', ''])
        sc = load(host + cam + pr + cam, scene_extra='window = [320, 180]' + nl + 'output = [160, 90]' + nl + self.MUSIC)
        L = sc.shots[0].layers[0]
        self.assertTrue(L["transform"])
        self.assertEqual([round(k[4], 2) for k in L["keys"]], [1.6, 1.0, 1.0, 1.3])
        colours = {"host": (255, 0, 0), "print": (0, 0, 255)}   # BGR0 frames: host blue, print red
        for shot in sc.all_shots():
            d = os.path.join(film.out_dir(sc), "shots", shot.name)
            os.makedirs(d, exist_ok=True)
            sink = film.FrameSink(os.path.join(d, "shot.mkv"), 320, 180, sc.fps)
            for k in range(shot.n):
                sink.write(bytes(colours[shot.name] + (0,)) * (320 * 180))
            sink.close()
        master, _ = film.encode(sc, keep_frames=True)

        def px(t, x=80, y=45):
            raw = subprocess.run(["ffmpeg", "-v", "error", "-ss", f"{t}", "-i", master, "-frames:v", "1", "-f", "rawvideo",
                                  "-pix_fmt", "rgb24", "-"], capture_output=True).stdout
            o = (y * 160 + x) * 3
            return tuple(raw[o:o + 3])
        host_px = px(0.2)
        self.assertGreater(host_px[2], 150)                      # before its first key: the host only
        held = px(1.0)
        self.assertGreater(held[0], 150)                          # held: the print (red) at the centre
        self.assertLess(held[2], 100)
        self.assertGreater(px(1.0, 80, 64)[1], 150)               # the print's ivory foot below it
        gone = px(1.9)
        self.assertGreater(gone[2], 150)                          # thrown off the frame: the host again
        # an image file as the source
        img = os.path.join(film.out_dir(sc), "still.png")
        write_png(img, 64, 36, bytes((0, 255, 0, 0)) * (64 * 36))
        sc2 = load(nl.join(['[[shot]]', 'name = "host"', 'length = "1 bar"', 'tick = 1000',
                            f'layers = [{{ image = "{img.replace(chr(92), "/")}", card = "polaroid", size = [0.4, 0.4], keys = {keys} }}]', '']) + cam,
                   scene_extra='output = [160, 90]' + nl + self.MUSIC)
        self.assertTrue(sc2.shots[0].layers[0]["hold"])

    def test_photomatic(self):
        """A stills pass at the film's timing: each still held from its own frame to the next, a shot with none a card."""
        import subprocess
        from capture import write_png
        cam = self.CAM
        sc = load('[[shot]]\nname = "a"\nlength = "2 beats"\ntick = 1000\n' + cam +
                  '[[shot]]\nname = "b"\nlength = "2 beats"\ntick = 1100\n' + cam,
                  scene_extra='output = [160, 90]\n' + self.MUSIC)
        d = os.path.join(film.out_dir(sc), "shots", "a_stills")
        os.makedirs(d, exist_ok=True)
        for k, rgb in ((0, (255, 0, 0)), (8, (0, 0, 255))):          # frame 0 red, frame 8 blue (BGRA)
            write_png(os.path.join(d, f"f{k:05d}.png"), 64, 36, bytes(rgb + (0,)) * (64 * 36))
        out = film.photomatic(sc, size=(160, 90))

        def px(t):
            raw = subprocess.run(["ffmpeg", "-v", "error", "-ss", f"{t}", "-i", out, "-frames:v", "1", "-f", "rawvideo",
                                  "-pix_fmt", "rgb24", "-"], capture_output=True).stdout
            o = (10 * 160 + 10) * 3
            return tuple(raw[o:o + 3])
        frames = subprocess.run(["ffprobe", "-v", "error", "-count_frames", "-select_streams", "v:0", "-show_entries",
                                 "stream=nb_read_frames", "-of", "csv=p=0", out], capture_output=True, text=True).stdout
        self.assertEqual(int(frames.strip()), 64)     # every frame of both shots (the concat route ran short)
        early, late, card = px(0.1), px(0.6), px(1.5)
        self.assertGreater(early[2], 150)             # the first still (written BGRA: blue on screen)
        self.assertGreater(late[0], 150)              # held until frame 8, then the second
        self.assertLess(max(card), 120)               # shot b has no stills: the grey card


    def test_short_forward_seek_is_stepped(self):
        # the freeze of the trailer (s20) followed the latch (s19, last frame 9719) and sought 9726: a 7-tick
        # forward seek plays on instead of skipping. Short forward moves from a known paused tick are stepped.
        self.assertEqual(film.plan_seek(9719, 9726), ("step", 7))
        self.assertEqual(film.plan_seek(9719, 9719), ("stay", 0))
        self.assertEqual(film.plan_seek(9719, 9719 + film.SHORT_SEEK), ("step", film.SHORT_SEEK))
        self.assertEqual(film.plan_seek(9719, 9720 + film.SHORT_SEEK), ("seek", 9720 + film.SHORT_SEEK))
        self.assertEqual(film.plan_seek(9871, 9744), ("seek", 9744))     # backward: a real seek
        self.assertEqual(film.plan_seek(None, 9726), ("seek", 9726))     # after a playdemo or a played shot


if __name__ == "__main__":
    unittest.main()
