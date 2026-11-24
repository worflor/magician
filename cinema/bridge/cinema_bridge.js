"use strict";
// cinema_dev bridge: Panorama JS that runs console commands and drives the camera once per rendered frame.
// Dev only: it ships in the cinema dev pak, never with a mod. Two layouts include it: the cinema copy of the replay
// HUD (panorama/layout/replay/citadel_hud_replay: demo playback on any map) and of the Testing Tools layout
// (hud_hero_testing: the Hero Sandbox and demos recorded there). Where both load, the first one to run keeps the
// keys (a flag on the root panel) so a key press is taken once. It takes no input from outside the game but its
// keys: no web requests (Deadlock removed $.AsyncWebRequest), no console events (custom event types don't exist).
//
//   F11  arm the first baked path (CINEMA_PATHS[0])          each arm logs "cinema_bridge: armed <i> <name>"
//   F9   arm the next one
//   F8   play the armed path, once per rendered frame; again: stop. A path with `clock: "frames"` is paced: one
//        sample a rendered frame (not by wall time), each frame `host_framerate <rates[k]>` with its camera, so the
//        demo moves exactly rates[k] seconds a rendered frame whatever the wall clock does; every frame logs
//        "cinema_bridge: frame <k> <name>" (k = n on the frame after the last). film.py slows the game (fps_max)
//        and captures one picture a rendered frame from those lines: slow motion between ticks, frame by frame.
//   F10  self-test: two marker commands and a 120-frame dolly of the override camera, frame times logged
//
// A path: {name, mode: "spec" | "override", fps, seconds, samples: [[x, y, z, pitch, yaw], ...], start, end}.
// Playing it, every rendered frame sets the camera to the samples interpolated at the time since play began (wall
// time: a realtime shot plays the demo at demo_timescale, so screen time is wall time). `start` runs on the first
// frame together with the first camera (film.py: "demo_resume"), `end` after the last ("demo_pause").
// Proven 2026-11-21: global key binds fire on injected keys (binds on the context panel never did), and
// $.DispatchEvent("CitadelConCommand", ...) runs a console command from JS, 120 in 0.98 s, one a frame.

var CinemaBridge = (function () {
	var ctx = $.GetContextPanel();
	var where = (ctx && ctx.id) ? ctx.id : "?";
	var playing = null;
	var armed = 0;
	var lastKey = {};

	function log(text) {
		$.Msg("cinema_bridge: " + text);
	}

	function cmd(line) {
		$.DispatchEvent("CitadelConCommand", ctx, line);
	}

	function paths() {
		return (typeof CINEMA_PATHS !== "undefined") ? CINEMA_PATHS : [];
	}

	// The camera at time t (seconds) of a path: samples interpolated linearly, yaw on the shortest arc.
	function sampleAt(path, t) {
		var f = t * path.fps;
		var n = path.samples.length;
		var i = Math.min(n - 1, Math.max(0, Math.floor(f)));
		var j = Math.min(n - 1, i + 1);
		var s = Math.min(1, Math.max(0, f - i));
		var a = path.samples[i], b = path.samples[j];
		var v = [];
		for (var k = 0; k < a.length; k++) {
			var d = b[k] - a[k];
			if (k === 4 && path.mode !== "override") {
				d = ((d % 360) + 540) % 360 - 180;
			}
			v.push(a[k] + d * s);
		}
		return v;
	}

	function lineAt(path, t) {
		var v = sampleAt(path, t);
		if (path.mode === "override") {   // x y z offsets and fov of the third-person camera
			return "citadel_camera_override_x_offset " + v[0].toFixed(2) + ";citadel_camera_override_y_offset " +
				v[1].toFixed(2) + ";citadel_camera_override_z_offset " + v[2].toFixed(2) +
				(v.length > 3 ? ";citadel_camera_override_fov " + v[3].toFixed(2) : "");
		}
		return "spec_goto " + v[0].toFixed(2) + " " + v[1].toFixed(2) + " " + v[2].toFixed(2) + " " +
			v[3].toFixed(2) + " " + (((v[4] % 360) + 360) % 360).toFixed(2);
	}

	function stop(why) {
		if (playing) {
			if (playing.path.end) {
				cmd(playing.path.end);
			}
			log("done " + playing.path.name + ": " + playing.frames + " frames in " + playing.t.toFixed(3) + " s (" +
				(playing.frames / Math.max(playing.t, 0.001)).toFixed(1) + " a second, worst gap " +
				(playing.worst * 1000).toFixed(1) + " ms; " + why + ")");
		}
		playing = null;
	}

	// paced: frame k of the path on the k-th rendered frame after a pre-roll and a warm-up, and the demo's time step
	// for it. The pre-roll (PREROLL frames of camera 0 at a near-zero step, the demo resumed on the first) lets the
	// resume take hold: in the first probe (2026-11-21) the demo only moved from the third frame after it. The warm-up
	// (path.warm frames of path.warm_rate seconds each, at the player's own frame rate) plays the demo up to the shot's
	// first tick from the seek's, so the cloth simulates continuously into frame 0 instead of starting from the seek's
	// teleport (Maid Lash's skirt stretched metres for two frames and lagged her legs after a seek). path.fps_max then
	// slows the game for the capture. The post-roll keeps logging frame lines (the last camera, a near-zero step) so the
	// capture, which trails the lines, can take the last frames.
	var PREROLL = 4;
	var POSTROLL = 6;
	var TINY = 0.000001;

	function tickFrames() {
		var p = playing.path;
		var warm = p.warm || 0;
		var n = p.samples.length;
		var k = playing.frames - PREROLL - warm;
		if (k >= n + POSTROLL) {
			log("frame " + k + " " + p.name + " (end)");
			stop("end");
			return;
		}
		if (k < -warm) {   // pre-roll
			cmd(lineAt(p, 0) + ";host_framerate " + TINY.toFixed(8));
			if (k === -warm - PREROLL && p.start) {
				cmd(p.start);
			}
		} else if (k < 0) {   // warm-up: the demo plays toward frame 0 at the warm-up step
			cmd(lineAt(p, 0) + ";host_framerate " + (p.warm_rate || 1 / p.fps).toFixed(8) +
				(k === -1 && p.fps_max ? ";fps_max " + p.fps_max : ""));
		} else if (k < n) {
			var rate = p.rates ? p.rates[k] : 1 / p.fps;
			cmd(lineAt(p, k / p.fps) + ";host_framerate " + Math.max(rate, TINY).toFixed(8) +
				(k === 0 && !warm && p.fps_max ? ";fps_max " + p.fps_max : ""));
			log("frame " + k + " " + p.name);
		} else {   // post-roll
			cmd(lineAt(p, (n - 1) / p.fps) + ";host_framerate " + TINY.toFixed(8));
			log("frame " + k + " " + p.name);
		}
		playing.frames++;
		$.Schedule(0.0, tick);
	}

	function tick() {
		if (!playing) {
			return;
		}
		if (playing.path.clock === "frames") {
			tickFrames();
			return;
		}
		var now = $.FrameTime();
		if (playing.t0 === null) {
			playing.t0 = now;
			playing.last = now;
			cmd(lineAt(playing.path, 0));
			if (playing.path.start) {
				cmd(playing.path.start);
			}
			playing.frames = 1;
			$.Schedule(0.0, tick);
			return;
		}
		var t = now - playing.t0;
		playing.worst = Math.max(playing.worst, now - playing.last);
		playing.last = now;
		playing.t = t;
		if (t >= playing.path.seconds) {
			stop("end");
			return;
		}
		cmd(lineAt(playing.path, t));
		playing.frames++;
		$.Schedule(0.0, tick);
	}

	function arm(index) {
		var list = paths();
		if (!list.length) {
			log("no baked paths");
			return;
		}
		armed = ((index % list.length) + list.length) % list.length;
		log("armed " + armed + " " + list[armed].name);
	}

	function play() {
		var list = paths();
		if (!list.length) {
			log("no baked paths");
			return;
		}
		if (playing) {
			stop("replaced");
		}
		var path = list[armed];
		playing = { path: path, t0: null, last: 0, t: 0, frames: 0, worst: 0 };
		log("play " + armed + " " + path.name + " (" + path.seconds + " s, " + path.samples.length + " samples at " +
			path.fps + " fps)");
		$.Schedule(0.0, tick);
	}

	function selfTest() {
		log("self-test: CitadelConCommand with the context panel, then without");
		cmd("citadel_camera_override_fov 61");
		$.DispatchEvent("CitadelConCommand", "citadel_camera_override_fov 62");
		var n = 0, t0 = $.FrameTime(), last = t0, worst = 0;
		cmd("citadel_camera_overrides_enabled 1");
		function step() {
			var now = $.FrameTime();
			worst = Math.max(worst, now - last);
			last = now;
			cmd("citadel_camera_override_x_offset " + (-300 + 3 * n).toFixed(1));
			n++;
			if (n < 120) {
				$.Schedule(0.0, step);
			} else {
				log("self-test: 120 commands over " + (now - t0).toFixed(3) + " s, worst frame gap " + (worst * 1000).toFixed(1) + " ms");
			}
		}
		$.Schedule(0.0, step);
	}

	// A key is bound twice (global, and on the panel in case a build fires only that one): one press in one frame
	// is taken once.
	function onKey(key, fn) {
		function once() {
			var now = $.FrameTime();
			if (lastKey[key] !== undefined && now - lastKey[key] < 0.001) {   // the same frame
				return;
			}
			lastKey[key] = now;
			fn();
		}
		try { $.RegisterKeyBind("", key, once); } catch (e) { log("global bind " + key + " failed: " + e); }
		try { $.RegisterKeyBind(ctx, key, once); } catch (e) { log("panel bind " + key + " failed: " + e); }
	}
	// one bridge per UI: the root panel carries a flag the first instance sets (another layout's copy stands down)
	function root() {
		var p = ctx;
		try {
			while (p && p.GetParent && p.GetParent()) {
				p = p.GetParent();
			}
		} catch (e) { }
		return p;
	}
	var top = root();
	try {
		if (top && top.GetAttributeInt && top.GetAttributeInt("cinema_bridge", 0) === 1) {
			log("already loaded elsewhere; this copy (" + where + ") stands down");
			return { play: play, stop: stop, arm: arm, selfTest: selfTest, cmd: cmd, lineAt: lineAt, sampleAt: sampleAt, standby: true };
		}
		if (top && top.SetAttributeInt) {
			top.SetAttributeInt("cinema_bridge", 1);
		}
	} catch (e) {
		log("no root flag (" + e + "): keys may be taken by more than one copy");
	}
	onKey("key_f8", function () { if (playing) { stop("key"); } else { play(); } });
	onKey("key_f9", function () { arm(armed + 1); });
	onKey("key_f11", function () { arm(0); });
	onKey("key_f10", selfTest);
	log("loaded in " + where + ", " + paths().length + " baked path(s); F11 arm first, F9 arm next, F8 play/stop, F10 self-test");

	return { play: play, stop: stop, arm: arm, selfTest: selfTest, cmd: cmd, lineAt: lineAt, sampleAt: sampleAt };
})();

if (typeof module !== "undefined") {
	module.exports = CinemaBridge;
}
