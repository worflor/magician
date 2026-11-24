// Offline test of the Panorama bridge: runs cinema_bridge.js against a stub of Panorama's $ with a fake frame
// clock, presses its keys, and checks the commands it would send each rendered frame.   node test_bridge.js
"use strict";
const fs = require("fs");
const path = require("path");
const vm = require("vm");
const assert = require("assert");

function load(paths, fps, ui) {
	const sent = [], msgs = [], keys = ui ? ui.keys : {}, queue = [];
	let now = 100.0;
	// a panel under a shared root (two layouts in one UI share the root and its attributes)
	const top = ui ? ui.root : { attrs: {}, GetParent: () => null,
		GetAttributeInt(k, d) { return k in this.attrs ? this.attrs[k] : d; }, SetAttributeInt(k, v) { this.attrs[k] = v; } };
	const panel = { id: ui ? ui.id : "ctx", GetParent: () => top };
	const $ = {
		GetContextPanel: () => panel,
		Msg: (m) => msgs.push(m),
		DispatchEvent: (ev, a, b) => { assert.strictEqual(ev, "CitadelConCommand"); sent.push({ frame: frames.n, line: b === undefined ? a : b }); },
		Schedule: (_t, fn) => queue.push(fn),
		FrameTime: () => now,
		RegisterKeyBind: (where, key, fn) => { (keys[key] = keys[key] || []).push(fn); },
	};
	const frames = { n: 0 };
	const ctx = vm.createContext({ $, CINEMA_PATHS: paths, Math, Number });
	vm.runInContext(fs.readFileSync(path.join(__dirname, "cinema_bridge.js"), "utf8"), ctx);
	return {
		sent, msgs,
		press: (k) => { for (const fn of keys[k] || []) fn(); now += 0.02; },   // both binds fire: taken once
		frame: (dt) => { now += dt === undefined ? 1 / fps : dt; frames.n++; const q = queue.splice(0); q.forEach((fn) => fn()); },
		bridge: ctx.CinemaBridge, root: top, keys,
	};
}

const path0 = { name: "a_orbit", mode: "spec", fps: 32, seconds: 1.0, start: "demo_resume", end: "demo_pause",
	samples: Array.from({ length: 32 }, (_, k) => [k * 10, 0, 50, 5, (350 + k * 2) % 360]) };
const path1 = { name: "b_dolly", mode: "spec", fps: 32, seconds: 0.5, start: "demo_resume", end: "demo_pause",
	samples: Array.from({ length: 16 }, (_, k) => [0, k, 60, 0, 90]) };

// arming: F11 first, F9 next; a key bound twice fires once
let t = load([path0, path1], 144);
t.press("key_f11");
t.press("key_f9");
assert.ok(t.msgs.some((m) => m.includes("armed 1 b_dolly")), "F9 arms the next path");
t.press("key_f9");
assert.ok(t.msgs[t.msgs.length - 1].includes("armed 0 a_orbit"), "arming wraps round, once per press: " + t.msgs.slice(-1));

// playing at 144 rendered frames a second: the first frame sets the camera to sample 0 and resumes the demo
t.press("key_f8");
t.frame();
assert.deepStrictEqual(t.sent.map((s) => s.line), ["spec_goto 0.00 0.00 50.00 5.00 350.00", "demo_resume"]);
for (let i = 0; i < 200 && !t.msgs.some((m) => m.startsWith("cinema_bridge: done")); i++) t.frame();
const lines = t.sent.map((s) => s.line);
assert.strictEqual(lines[lines.length - 1], "demo_pause", "the path ends with its end command");
const gotos = lines.filter((l) => l.startsWith("spec_goto"));
assert.ok(gotos.length >= 143 && gotos.length <= 145, "one camera a rendered frame for 1 s at 144 fps: " + gotos.length);
// interpolation between samples, and yaw across 360 on the shortest arc
assert.strictEqual(t.bridge.lineAt(path0, 0.5 / 32), "spec_goto 5.00 0.00 50.00 5.00 351.00");
assert.strictEqual(t.bridge.lineAt(path0, 5.5 / 32), "spec_goto 55.00 0.00 50.00 5.00 1.00", "350+2k wraps past 0");
assert.strictEqual(t.bridge.lineAt(path0, 99), "spec_goto 310.00 0.00 50.00 5.00 52.00", "held at the last sample");
// every camera line is monotonic in x: no frame goes back (no repeated or reordered samples)
const xs = gotos.map((l) => Number(l.split(" ")[1]));
for (let i = 1; i < xs.length; i++) assert.ok(xs[i] >= xs[i - 1], "x goes forward");
assert.ok(t.msgs.some((m) => /done a_orbit: \d+ frames/.test(m)));

// F8 while playing stops it (and still sends the end command)
t = load([path0], 60);
t.press("key_f8");
t.frame(); t.frame();
t.press("key_f8");
assert.strictEqual(t.sent[t.sent.length - 1].line, "demo_pause");
assert.ok(t.msgs.some((m) => m.includes("done a_orbit") && m.includes("key")));

// two layouts load the bridge in one UI (the replay HUD and Testing Tools): the second stands down, a key is taken once
t = load([path0, path1], 60);
const second = load([path0, path1], 60, { root: t.root, keys: t.keys, id: "hud_hero_testing" });
assert.ok(second.msgs.some((m) => m.includes("stands down")), "the second copy stands down: " + second.msgs);
t.press("key_f11");
t.press("key_f9");
assert.strictEqual(t.msgs.filter((m) => m.includes("armed")).length, 2, "one arm a press");
assert.ok(t.msgs[t.msgs.length - 1].includes("armed 1 b_dolly"));

// paced (clock "frames"): a pre-roll of 4 frames (camera 0, a near-zero step, the demo resumed on the first), then
// sample k and host_framerate rates[k] on the k-th rendered frame whatever the frame time; frame n logs the end and
// sends the end command; a rate of 0 is a near-frozen step
const paced = { name: "d_paced", mode: "spec", fps: 32, seconds: 0.125, clock: "frames", start: "demo_resume", end: "demo_pause",
	rates: [0, 0.0078125, 0.0078125, 0], samples: Array.from({ length: 4 }, (_, k) => [k, 0, 50, 0, 90]) };
t = load([paced], 60);
t.press("key_f11");
t.press("key_f8");
t.frame(0.3);
assert.deepStrictEqual(t.sent.map((s) => s.line), ["spec_goto 0.00 0.00 50.00 0.00 90.00;host_framerate 0.00000100", "demo_resume"]);
t.frame(); t.frame(); t.frame();
assert.ok(!t.msgs.some((m) => / frame /.test(m)), "no frame lines during the pre-roll");
t.frame(0.001);
assert.strictEqual(t.sent[t.sent.length - 1].line, "spec_goto 0.00 0.00 50.00 0.00 90.00;host_framerate 0.00000100", "frame 0");
t.frame(2.0);
assert.strictEqual(t.sent[t.sent.length - 1].line, "spec_goto 1.00 0.00 50.00 0.00 90.00;host_framerate 0.00781250", "frame 1 whatever the frame time");
t.frame(); t.frame();
assert.strictEqual(t.sent[t.sent.length - 1].line, "spec_goto 3.00 0.00 50.00 0.00 90.00;host_framerate 0.00000100");
for (let i = 0; i < 6; i++) t.frame();   // the post-roll: the last camera, still logging frame lines
assert.ok(t.msgs.some((m) => / frame 9 d_paced$/.test(m)), "post-roll lines");
t.frame();
assert.strictEqual(t.sent[t.sent.length - 1].line, "demo_pause");
assert.ok(t.msgs.some((m) => m.includes("frame 10 d_paced (end)")));

// the warm-up: from the seek, warm frames at warm_rate (the cloth settles), fps_max set on the last one, then frame 0
const warmed = Object.assign({}, paced, { name: "e_warm", warm: 3, warm_rate: 0.03125, fps_max: 6 });
t = load([warmed], 60);
t.press("key_f11");
t.press("key_f8");
for (let i = 0; i < 4; i++) t.frame();
t.sent.length = 0;
t.frame(); t.frame(); t.frame();
assert.deepStrictEqual(t.sent.map((s) => s.line), ["spec_goto 0.00 0.00 50.00 0.00 90.00;host_framerate 0.03125000",
	"spec_goto 0.00 0.00 50.00 0.00 90.00;host_framerate 0.03125000",
	"spec_goto 0.00 0.00 50.00 0.00 90.00;host_framerate 0.03125000;fps_max 6"]);
assert.ok(!t.msgs.some((m) => / frame /.test(m)), "no frame lines during the warm-up");
t.frame();
assert.ok(t.msgs.some((m) => / frame 0 e_warm$/.test(m)));

// no paths baked: nothing sent, a clear message
t = load([], 60);
t.press("key_f8");
assert.ok(t.msgs.some((m) => m.includes("no baked paths")));
assert.strictEqual(t.sent.length, 0);

// the paths a build baked (when there is one): every realtime shot arms by F11 + F9s and plays to its end
const baked = path.join(__dirname, "..", "addon", "panorama", "scripts", "cinema", "cinema_paths.js");
if (fs.existsSync(baked)) {
	const sandbox = vm.createContext({});
	vm.runInContext(fs.readFileSync(baked, "utf8").replace("var CINEMA_PATHS", "this.CINEMA_PATHS"), sandbox);
	const list = sandbox.CINEMA_PATHS;
	list.forEach((p, i) => {
		const b = load(list, 120);
		b.press("key_f11");
		for (let k = 0; k < i; k++) b.press("key_f9");
		assert.ok(b.msgs[b.msgs.length - 1].includes("armed " + i + " " + p.name), "arms " + p.name);
		b.press("key_f8");
		for (let f = 0; f < 120 * (p.seconds + 1) && !b.msgs.some((m) => m.includes("done " + p.name)); f++) b.frame();
		assert.ok(b.msgs.some((m) => m.includes("done " + p.name)), p.name + " plays to its end");
		assert.ok(p.samples.every((v) => v.length === 5 && v.every(Number.isFinite)), p.name + " samples are finite");
	});
	console.log("bridge: " + list.length + " baked path(s) arm and play: " + list.map((p) => p.name).join(", "));
}

console.log("bridge: all tests passed");
