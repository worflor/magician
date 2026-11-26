//! Magician lint: deterministic checks that a mod VPK will behave in-game the way its author thinks.
//!
//! Every rule states what actually happens in the engine when it fires, not just what is "wrong".
//! Every rule has a fixture in `tests` built to trigger exactly that rule. A rule without a fixture
//! that proves it fires is a wish, not a check.

#![forbid(unsafe_code)]

pub mod allow;
pub mod baseline;
pub mod cache;
pub mod cloth;
pub mod corpus;
pub mod deep;
pub mod distfield;
pub mod matconst;
pub mod pbrcombo;
pub mod policy;
pub mod report;
pub mod sources;
pub mod mesh;
pub mod par;
pub use baseline::{Baseline, FileInfo};

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub code: &'static str,
    pub name: &'static str,
    pub severity: Severity,
    pub path: Option<String>,
    pub message: String,
}

pub struct Rule {
    pub code: &'static str,
    pub name: &'static str,
    pub severity: Severity,
    /// What the player sees when this goes wrong.
    pub in_game: &'static str,
}

/// The revision of how the rules judge, beyond their codes, names and severities: raised whenever a rule's test or a
/// finding's own severity changes (a narrower test, a finding the files show to have no consequence). The update
/// check's lint fingerprint carries it, so a user is told to re-run lint. 2: the first published mods by others,
/// witnessed in game (CHANGELOG, "judgement changed").
pub const JUDGEMENT: u32 = 2;

/// The rule catalogue: one home for codes, names and in-game consequences.
pub const RULES: &[Rule] = &[
    Rule { code: "M001", name: "path-case", severity: Severity::Error,
        in_game: "Source 2 looks paths up in lowercase. A path with capitals is never found, so the original asset shows instead. Not said of a file the game never reads (M012): its case costs nothing." },
    Rule { code: "M002", name: "path-shape", severity: Severity::Error,
        in_game: "Backslashes, leading slashes or '..' in a VPK path never match an engine lookup; the file is dead weight." },
    Rule { code: "M010", name: "uncompiled-source", severity: Severity::Error,
        in_game: "The game only loads compiled resources (*_c). A source .vmdl/.vmat/.vpcf in a VPK is ignored." },
    Rule { code: "M011", name: "loose-media", severity: Severity::Warn,
        in_game: "Raw images and audio (.png, .tga, .wav, .mp3, .psd) are never read by the game; they must be compiled to .vtex_c/.vsnd_c." },
    Rule { code: "M012", name: "noise-file", severity: Severity::Info,
        in_game: "Not read by Deadlock (addoninfo.txt, readmes, editor files). Harmless, but it adds size and conflict noise." },
    Rule { code: "M020", name: "crc-mismatch", severity: Severity::Error,
        in_game: "The stored checksum doesn't match the bytes: a corrupt or hand-patched VPK. Mod managers flag it, and the engine may reject it." },
    Rule { code: "M021", name: "not-a-resource", severity: Severity::Error,
        in_game: "A *_c file that isn't a valid compiled resource fails to load: error model, pink checkerboard, or silence." },
    Rule { code: "M030", name: "dangling-ref", severity: Severity::Error,
        in_game: "This resource depends on a file that exists in neither the mod nor the game build: missing material (pink checkerboard), error model, or missing effect." },
    Rule { code: "M070", name: "ships-protected-file", severity: Severity::Error,
        in_game: "Skeletons (.vnmskel), anim graphs (.vnmgraph) and game data (.vdata) replace the hero's real files. It works until the next patch, then the hero breaks. A model replacement keeps Valve's skeleton and graph." },
    Rule { code: "M071", name: "replaces-engine-default", severity: Severity::Error,
        in_game: "resourcecompiler emits its own copies of the engine's default textures (materials/default/*) next to your materials, and they differ from Valve's. Shipped with other pixels than Valve's, they replace the fallback texture for every material in the game that uses one (an error). A copy that decodes to Valve's own pixels, or sits at a path Valve does not ship, changes nothing in the game and is a warning (seen on three published mods, 6868)." },
    Rule { code: "M061", name: "dropped-dependency", severity: Severity::Warn,
        in_game: "The game's original of this file referenced something that plays (a particle, a sound or sound event, an anim graph or clip, a sound snapshot) and your replacement no longer does: that effect or sound silently stops playing. A material or texture the original drew with and yours does not is a restyle, not a loss: the replacement draws with its own (and the compiler names a texture after its settings, so the same art recompiled has other names)." },
    Rule { code: "M032", name: "stale-leftover", severity: Severity::Error,
        in_game: "A file the mod references has no source left in the addon: a compiled leftover of something deleted. It ships the old content (one release shipped five deleted effects this way, one a shock ring at 1.3x Valve's brightness) and no rebuild can ever change it." },
    Rule { code: "M033", name: "dev-leftover", severity: Severity::Warn,
        in_game: "A particle, material or file whose path marks it as debug content (probe, debug, test, tmp, scratch...) ships in the release. It is a test aid, not art: a probe child spawned on every cast (one release shipped one), a placeholder swatch, an effect nobody meant to show. If it is also a dangling reference, M030/M032 already report that as an error. A material or texture the mod draws with is not reported: it is the art on screen, whatever the exporter named it." },
    Rule { code: "M031", name: "orphan-new-file", severity: Severity::Warn,
        in_game: "The file doesn't replace anything in the game, and nothing in the mod references it, so it never loads." },
    Rule { code: "M040", name: "drift-original-changed", severity: Severity::Warn,
        in_game: "Valve changed the original of a file you replace since you built against it. Your copy may now be missing fixes, attachments or new animations." },
    Rule { code: "M041", name: "drift-original-removed", severity: Severity::Error,
        in_game: "The file you replace no longer exists in this build (moved or renamed), so your replacement does nothing." },
    Rule { code: "M050", name: "vpk-name", severity: Severity::Warn,
        in_game: "Addon VPKs are mounted as pakNN_dir.vpk (NN = 01..99, lowest wins). Other names are skipped by the addons search path and by mod managers' load-order logic." },
    Rule { code: "M051", name: "vpk-chunked", severity: Severity::Warn,
        in_game: "Data stored in chunk files (_000.vpk...) is easy to ship incomplete. Single-file VPKs are the safe form for addons." },
    Rule { code: "M052", name: "vpk-empty", severity: Severity::Error,
        in_game: "An empty VPK changes nothing." },
    Rule { code: "M013", name: "build-artifact", severity: Severity::Warn,
        in_game: "A build input or toolchain byproduct (a panorama image list, a nested VPK, a raw .kv3, a compiler cache) that the game never loads from an addon. It adds size, and a vdata among them is noted in the grade. magician build leaves these out." },
    Rule { code: "M062", name: "model-contract", severity: Severity::Error,
        in_game: "A replaced model lost or moved a bone, attachment or hitbox of Valve's, or added a hitbox. Effects and weapons attach to the wrong place (or the origin), animation breaks, and changed hitboxes change what bullets hit: a fair-play break." },
    Rule { code: "M063", name: "sound-cue-reduced", severity: Severity::Error,
        in_game: "A replaced sound event or file plays quieter, shorter, over less distance, to fewer players, or not at all compared with Valve's. Enemies rely on those cues: hiding them is a fair-play break. A changed base is read through the game's own base events: one Valve's base inherits from differs only by what lies between, and only a level or a reach there is a finding (louder is M064). A voice line (sounds/vo/) is not judged by its length: Valve's own voices fail that test against each other on 46% of shared lines (`magician corpus`)." },
    Rule { code: "M064", name: "sound-louder", severity: Severity::Warn,
        in_game: "A replaced sound plays louder, longer or carries further than Valve's. Not an advantage, but it pushes the hero over the game's mix and masks other players' cues." },
    Rule { code: "M072", name: "light-cookie-texture", severity: Severity::Error,
        in_game: "A light cookie (gobo) the engine can't add to its light texture set, so the light renders without its pattern. The set takes BGRA8888 only ('Unable to AddTextureToSet(...): wrong format DXT1 - expected BGRA8888!') and copies a 512x512 mip ('... has no mip levels of size 512x512'). Valve's 15 cookies are all 512x512 BGRA8888: set m_outputFormat \"BGRA8888\" and author at 512." },
    Rule { code: "M073", name: "texture-not-pow2", severity: Severity::Warn,
        in_game: "A model or effect texture whose sides aren't powers of two. Valve ships none under models/ (4 of 12,598 outside panorama and UI, all particle sheets); the compiler refuses non-power-of-two roughness outright ('Cannot filter non-power-of-two texture 74x74'), and other slots lose mips and filter badly at distance." },
    Rule { code: "M081", name: "particle-energy", severity: Severity::Warn,
        in_game: "A replaced particle is much brighter (colour x alpha x overbright) or larger than Valve's original. Bigger, brighter effects cover more of the screen and of the hero: Maid Lash's first pass hid him behind a dive streak with 3.4x the coverage and fired a 9x brighter muzzle flash." },
    Rule { code: "M082", name: "particle-light-caps", severity: Severity::Error,
        in_game: "A replaced particle tree has more lights than Valve's, brighter or longer-range lights, a colour with more luminous output, or shadows switched on. Lights wash out the area around the hero and the people in it (Maid Lash's gobo at 2.7x)." },
    Rule { code: "M083", name: "particle-class-list", severity: Severity::Error,
        in_game: "A particle function sits in a list whose registry doesn't have its class: the game logs 'Function \"...\" not found while loading particle system'. A constraint (C_OP_WorldTraceConstraint) in m_Operators crashed the game outright: an access violation executing a null pointer in particles.dll the first time the effect played. Valve's 133 uses of that class all sit in m_Constraints." },
    Rule { code: "M090", name: "shared-file", severity: Severity::Warn,
        in_game: "The game file this replaces is used by more than one hero, ability or item (Lash's Ground Strike icon is also Majestic Leap's). Replacing it changes all of them for everyone who installs the mod." },
    Rule { code: "M091", name: "shared-file-unchecked", severity: Severity::Info,
        in_game: "Deep lint had no users index for this build (or one from an older Magician), so the files this mod replaces were not checked for other heroes, abilities or items that use them (M090). A shared file it replaces changes those too; unchecked is not clean. `magician index` builds the index, once per build." },
    // The mesh family (M100-M140, M145-M147): compiled models decoded natively, measured with the geometry kernel,
    // judged against Valve's model at the same path (`mesh/`). Derived from scene3d's census and lint.
    Rule { code: "M100", name: "mesh-unchecked", severity: Severity::Warn,
        in_game: "Part of the model could not be measured (a buffer that doesn't decode, a capped search). Silence there is not evidence: those rules did not run." },
    Rule { code: "M101", name: "mesh-non-finite", severity: Severity::Error,
        in_game: "Vertex positions that are NaN or infinite: triangles spike to infinity across the screen." },
    Rule { code: "M102", name: "open-sheet-void", severity: Severity::Warn,
        in_game: "An opening in a single-sided piece lets you look at its inside, which the game culls: from that angle you see straight through it (a train, a hem, a sleeve)." },
    Rule { code: "M103", name: "winding-inconsistent", severity: Severity::Warn,
        in_game: "Neighbouring faces disagree on winding: one of them faces inward, so it is culled from outside or lit inside-out." },
    Rule { code: "M104", name: "normals-flipped", severity: Severity::Warn,
        in_game: "Shading normals point against their faces: visible triangles shade dark or flat, as if lit from behind." },
    Rule { code: "M105", name: "degenerate-triangles", severity: Severity::Warn,
        in_game: "Zero-area and sliver triangles far above Valve's rate: slivers shade and skin badly, zero-area ones flicker as dots." },
    Rule { code: "M106", name: "cut-body-void", severity: Severity::Warn,
        in_game: "Body faces were cut away under a garment and nothing backs the cut: wherever the garment lifts or the camera finds a gap, you see through the hero (the bodice-back voids). A warning: it is seen, not a model that fails to work, and on a published mod (6868) nine openings behind the face and at the neck were not visible from the gameplay camera at any pitch or close behind the head." },
    Rule { code: "M107", name: "backface-shading", severity: Severity::Warn,
        in_game: "Back faces of a two-sided material are what you see from outside: their normals face away, so they shade flat." },
    Rule { code: "M108", name: "z-fighting", severity: Severity::Warn,
        in_game: "Two pieces share a plane closer than the depth buffer can separate (0.3 mm over 1 cm^2): shimmering stripes as the camera moves." },
    Rule { code: "M109", name: "self-intersection", severity: Severity::Warn,
        in_game: "A piece folds through itself at bind: its layers flicker along the fold." },
    Rule { code: "M110", name: "shell-intersection", severity: Severity::Warn,
        in_game: "Two pieces pass through each other at bind (a garment through the body, a spool through the bodice): a visible clip that flickers along the line." },
    Rule { code: "M111", name: "floating-shell", severity: Severity::Warn,
        in_game: "A piece touches nothing it should rest on, or rests on a surface but is skinned to other bones: it hovers with a gap, or lifts off as soon as the hero moves (the arm band)." },
    Rule { code: "M112", name: "coordinate-precision", severity: Severity::Warn,
        in_game: "A piece is so thin for its distance from the origin that float32 cannot hold its shape: it collapses into flickering slivers." },
    Rule { code: "M113", name: "uv-degenerate", severity: Severity::Warn,
        in_game: "Textured triangles with no UV area inside real UV islands: one texel smears across each." },
    Rule { code: "M114", name: "uv-overlap", severity: Severity::Info,
        in_game: "Texels claimed twice, beyond Valve's own rate: the same paint shows in two places (fine for deliberate mirroring)." },
    Rule { code: "M115", name: "uv-stretch", severity: Severity::Info,
        in_game: "UVs stretched over 4:1 on much of a material: the texture smears along the stretch." },
    Rule { code: "M116", name: "skin-unnormalised", severity: Severity::Error,
        in_game: "Vertex weights that don't sum to 1: those vertices shrink toward the model origin when the hero moves." },
    Rule { code: "M117", name: "skin-influences", severity: Severity::Warn,
        in_game: "More influences than the mesh stores, or many noise-level weights: dropped influences pop, tiny ones jitter." },
    Rule { code: "M118", name: "skin-distant-bone", severity: Severity::Warn,
        in_game: "Vertices weighted to a bone far beyond where Valve's model ever weights it: they stretch toward that bone when it moves." },
    Rule { code: "M119", name: "skin-unweighted", severity: Severity::Error,
        in_game: "Skinned vertices with no usable influence, or influences on bones the skeleton lacks: they stay behind while the body moves." },
    Rule { code: "M120", name: "lod-missing", severity: Severity::Warn,
        in_game: "Fewer LOD levels than Valve's model: at range the engine keeps drawing the heavy mesh." },
    Rule { code: "M121", name: "lod-silhouette", severity: Severity::Warn,
        in_game: "A lower LOD's silhouette departs from LOD0 more than Valve's does: the hero visibly pops at the switch distance." },
    Rule { code: "M122", name: "lod-budget", severity: Severity::Warn,
        in_game: "Vertex economy per LOD worse than Valve's (vertices per triangle, LOD triangles, a lower LOD keeping over 1.5x the share of LOD0's vertices that Valve's keeps): GPU cost at range with no visible gain." },
    Rule { code: "M123", name: "mesh-budget", severity: Severity::Warn,
        in_game: "LOD0 triangles, draw calls, bones or texture memory well over Valve's model: frame cost in team fights." },
    Rule { code: "M124", name: "silhouette-fair-play", severity: Severity::Error,
        in_game: "Our in-game silhouette is smaller than Valve's in a view: the hero is harder to see than the original, a fair-play break." },
    Rule { code: "M125", name: "seam-crack", severity: Severity::Warn,
        in_game: "Boundary vertices of neighbouring pieces sit a hair apart without welding: a hairline crack shows the background along the seam." },
    Rule { code: "M126", name: "texture-without-uv", severity: Severity::Error,
        in_game: "A draw call uses a textured material but has no UVs: the whole piece samples one texel." },
    Rule { code: "M127", name: "unreferenced-vertices", severity: Severity::Info,
        in_game: "Vertices in LOD0's buffers that no triangle uses: memory with no picture." },
    Rule { code: "M128", name: "material-unused", severity: Severity::Info,
        in_game: "The default material group lists a material no draw call uses: dead weight in the model's references." },
    Rule { code: "M129", name: "hitbox-envelope", severity: Severity::Error,
        in_game: "Visible surface lies more than 18 units beyond every hitbox (capsule, sphere or box of the default set), or over 3.5% of it lies beyond 10 units, counting only what Valve's own model at this path does not already have in the same hitbox region: enemies aim at cloth that can't be hit and the silhouette reads as excessive, 'a bit much' (Magician's fair-play line, the defaults of a policy). Never allowable: an allowance does not waive it, and the grade's fair play fails (NSFR)." },
    Rule { code: "M130", name: "hitbox-envelope-wide", severity: Severity::Warn,
        in_game: "Over 2% of the surface lies more than 5 units beyond every hitbox, beyond what Valve's own model has there: enemies aim at cloth that can't be hit. Precedent: Lady Geist's long dress, 0.6% with a worst point of 5.8 units." },
    Rule { code: "M131", name: "status-proxy-missing", severity: Severity::Error,
        in_game: "A hero material without F_USE_STATUS_EFFECTS_PROXY where every one of Valve's has it: the piece stays visible under invisibility and misses damage flashes." },
    Rule { code: "M132", name: "npr-lighting-missing", severity: Severity::Warn,
        in_game: "An opaque hero material without F_USE_NPR_LIGHTING where Valve's all use it: the piece is lit differently from the rest of the hero." },
    Rule { code: "M133", name: "alpha-test-outline", severity: Severity::Warn,
        in_game: "An alpha-tested material with a non-zero NPR outline mask: the outline traces every cut-out, turning lace into a mesh of dark lines." },
    Rule { code: "M134", name: "particle-prop-unlit", severity: Severity::Warn,
        in_game: "A lit opaque pbr.vfx material drawn by a particle's C_OP_RenderModels without NPR or self-illum: particle models get no normal lighting, so it renders black." },
    Rule { code: "M135", name: "texture-parity", severity: Severity::Warn,
        in_game: "A replaced texture has fewer mips, another format or another size than Valve's file: Valve's panels and materials draw it at Valve's sizes, so it aliases or shows block artefacts." },
    Rule { code: "M136", name: "albedo-range", severity: Severity::Warn,
        in_game: "Over 10% of the new materials' area sits outside Valve's own albedo range for this model: the piece reads brighter or darker than everything Valve drew, flattening or blooming in the game's lighting." },
    Rule { code: "M137", name: "albedo-absolute", severity: Severity::Error,
        in_game: "Over 5% of the area is absolute black (under 0.003 linear) or white (over 0.80): blacks lose every fold and read as holes, whites flatten and bloom (Valve's art guide: no absolute white or black)." },
    Rule { code: "M138", name: "alpha-coverage-drift", severity: Severity::Warn,
        in_game: "An alpha-tested material whose cut-out coverage changes by more than 30% on over 10% of its tiles at the mip the game samples at 8 m: lace turns solid or vanishes as it recedes, and sparkles in motion." },
    Rule { code: "M139", name: "alpha-coverage-gameplay", severity: Severity::Error,
        in_game: "The same coverage drift already at the gameplay distance (4 m): the lace visibly changes density on screen in normal play." },
    Rule { code: "M140", name: "hitbox-envelope-taste", severity: Severity::Warn,
        in_game: "Visible surface lies more than 10 units beyond every hitbox, beyond what Valve's own model has there and short of M129's line: enemies aim at cloth that can't be hit. Tasteful overflow is allowed: accept it with a written reason in magician.allow if the silhouette piece earns it." },
    Rule { code: "M141", name: "cloth-flutter", severity: Severity::Warn,
        in_game: "Crunchy fabric: a cloth group that rings for many swings at or above the body's own rhythm (the compiled goal, damping and gravity judged as a damped oscillator at the solver's step) flutters like paper-thin silk the moment the hero moves, however heavy the mesh looks, and with no collision radius or shape on its layers it passes through the body. A group with no damper at all (vertex attraction 0, what a goal-0 joint compiles to) keeps the whole of its swing every step and never settles, however slowly it sways. Valve's own numbers are not blamed unless the mod hangs a bigger mesh on them." },
    Rule { code: "M142", name: "distance-field-dropped", severity: Severity::Warn,
        in_game: "Valve's model carries distance fields (one voxel grid per bone, the ambient occlusion the body casts on itself and the world) and ours has none or fewer: the model loses that occlusion and its contact shadowing, silently (a model built from a Source 2 Viewer decompile always drops them). Fair-play neutral: accept it with a reason in magician.allow." },
    Rule { code: "M143", name: "pbr-combo-invalid", severity: Severity::Error,
        in_game: "A pbr material whose static combo breaks the shader's own rules (F_SHEEN with F_DETAIL, F_GLASS, F_CLOAK, F_TRANSLUCENT, F_UNLIT; F_DETAIL with F_UNLIT; the status proxy without NPR, unlit or self-illum or with F_CLOAK; front-face normals without alpha test and backfaces): the compiler exits 0 and keeps the flags but silently drops the inputs of one feature (F_SHEEN + F_DETAIL loses g_tDetail), and the game has no such shader variant, so the material renders without it." },
    Rule { code: "M144", name: "fair-play-unchecked", severity: Severity::Warn,
        in_game: "A fair-play comparison could not run on this file: its data, or Valve's original at the same path, did not decode (a new format after a patch, a damaged file), or the model gave nothing to measure. Whether it hides a cue, shrinks the silhouette or overflows the hitboxes is unknown, and unknown is never a pass: the grade's fair play reads FP? until it runs." },
    Rule { code: "M145", name: "inside-out-shell", severity: Severity::Warn,
        in_game: "A closed, single-sided piece whose signed volume is negative (and that is over a millimetre thick on average, twice its volume over its area): every face points inward. The game culls it from outside and draws its far inner wall instead, so it reads hollow or flat black (one skin's sleeves). An inverted-hull outline (a slightly larger copy around another piece, wound inside out to draw a line) is not culled into a line in game: it draws an opaque black shape over what it wraps, though Blender and Source 2 Viewer show the line; the finding names it and points to the material's own outline (F_SOLID_COLOR_OUTLINE, F_OVERRIDE_NPR_OUTLINE). Winding and normals agree with each other, so neither M103 nor M104 sees it. Valve's own inside-out pieces at the same place are not blamed." },
    Rule { code: "M146", name: "lod-inversion", severity: Severity::Warn,
        in_game: "A material has more triangles at a lower LOD than at the one before it (over 1% and 32 triangles, unless Valve's does the same there), or a lower LOD reaches more than 0.5 units (two silhouette pixels) outside LOD0's bounding box, beyond Valve's own reach: the piece gains detail as it recedes, or a decimated vertex spikes out of the mesh, and either pops at the switch distance." },
    Rule { code: "M147", name: "texture-mip-starved", severity: Severity::Warn,
        in_game: "A new texture on the model whose mip chain stops early: at 8 m its smallest stored mip still puts over 2 texels on each screen pixel (from the texture's size, its mip count and the UV density on the mesh), so the sampler skips texels and the surface shimmers as the hero or the camera moves." },
    Rule { code: "M148", name: "default-slot-literal", severity: Severity::Warn,
        in_game: "A replaced material whose texture slot Valve leaves to an engine default (materials/default/...) now holds a one-colour texture more than half the range away from that default's value: the whole surface reads the new value instead. A Source 2 Viewer decompile prints such defaults as literal colours, often the wrong one, and the recompile bakes it in silently (a glass mask of 0 for Valve's 1 drew a summon solid with the hero invisible inside it). Valve's own file is never blamed; a slot the mod fills with a real texture is its own design." },
    Rule { code: "M149", name: "material-texture-dropped", severity: Severity::Warn,
        in_game: "A replaced material lacks a texture slot Valve's material at the same path sets (a decompile and recompile drops one it did not understand): the game's code still samples it, logs 'Texture g_tAltColor_Reveal doesn't exist in ...vmat!' and 'returning error texture', and draws the error texture wherever that slot is used. Seen in game on a published mod's bow (50 error textures in one pass, none in vanilla), which no other rule reported." },
    Rule { code: "M060", name: "model-document-replaced", severity: Severity::Info,
        in_game: "Replacing a .vmdl_c replaces the WHOLE model document: hitboxes, attachments, cloth, LODs and anim events come from your file. Run the contract check against the original." },
];

pub fn rule(code: &str) -> &'static Rule {
    RULES.iter().find(|r| r.code == code).expect("unknown rule code")
}

fn finding(code: &'static str, path: Option<&str>, message: String) -> Finding {
    let r = rule(code);
    Finding { code: r.code, name: r.name, severity: r.severity, path: path.map(str::to_string), message }
}

/// One file inside the mod being linted.
#[derive(Clone)]
pub struct ModFile {
    pub path: String,
    pub stored_crc: u32,
    pub inline: bool,
    pub data: Vec<u8>,
}

/// Read access to the game build a mod is checked against, for rules that compare with Valve's originals.
pub trait Game: Sync {
    /// The bytes of a game file (the original a mod file replaces, or one a mod file references).
    fn read(&self, path: &str) -> Option<Vec<u8>>;
    /// Who in the game uses `path`: one entry per hero, ability or item that reaches it, from the build's
    /// users index (`magician index`). Empty when there is no index.
    fn users(&self, _path: &str) -> Vec<String> {
        Vec::new()
    }
    /// Why `users` knows nothing, when it does not: no users index for the build, or one in an older format. Then
    /// the shared-file rule (M090) cannot run, and M091 says so. `None`: the index was read.
    fn users_unchecked(&self) -> Option<String> {
        None
    }
    /// The particle classes Valve uses in each function list across the build (cached per build). `None` when
    /// unavailable: then M083 can't run.
    fn particle_classes(&self) -> Option<&deep::ParticleClasses> {
        None
    }
    /// The addon content trees the mod was compiled from (relative paths, forward slashes, lowercase), when lint
    /// is told them (`--sources` / `--addon-root`). `None`: M032 can't run.
    fn addon_sources(&self) -> Option<&std::collections::HashSet<String>> {
        None
    }
    /// What the game files are, for keys of cached results: changes whenever the installed game does.
    fn identity(&self) -> String {
        String::new()
    }
}

pub struct Input<'a> {
    /// File name of the VPK, e.g. "pak03_dir.vpk".
    pub vpk_name: String,
    pub files: Vec<ModFile>,
    /// The game build the mod is being checked against.
    pub base: Option<&'a Baseline>,
    /// The game build the mod was authored against (from its manifest), for drift checks.
    pub built_against: Option<&'a Baseline>,
    /// For each mod file that replaces a game file: the compiled paths the game's original references.
    /// Valve ships some references that resolve to nothing (dev manifests, unused UI); those must not be
    /// blamed on the mod, and references the original had but the replacement lacks are dropped dependencies.
    pub original_refs: &'a HashMap<String, Vec<String>>,
    /// The game's files, for comparisons with Valve's originals (fair play, contracts, shared files).
    pub game: Option<&'a dyn Game>,
    /// Run the full mesh family (geometry census of every model with embedded meshes, ours and
    /// Valve's: ~20-40 s a hero). Off: the cheap mesh checks only (budgets, LODs, materials).
    pub deep: bool,
    /// The fair-play limits in force (`policy::DEFAULT` unless a policy file moved them).
    pub fair_play: &'a policy::FairPlay,
}

static TIMINGS: std::sync::Mutex<Vec<(String, f64)>> = std::sync::Mutex::new(Vec::new());

/// Seconds spent per rule family in this process's lint runs (`magician lint --timings`).
pub fn timings() -> Vec<(String, f64)> {
    TIMINGS.lock().map(|t| t.clone()).unwrap_or_default()
}

fn record(family: &str, t: std::time::Instant) {
    if let Ok(mut v) = TIMINGS.lock() {
        let s = t.elapsed().as_secs_f64();
        match v.iter_mut().find(|x| x.0 == family) {
            Some(x) => x.1 += s,
            None => v.push((family.to_string(), s)),
        }
    }
}

/// Shared empty map for callers with no original data.
pub fn no_originals() -> &'static HashMap<String, Vec<String>> {
    static E: std::sync::OnceLock<HashMap<String, Vec<String>>> = std::sync::OnceLock::new();
    E.get_or_init(HashMap::new)
}

/// Compiled reference targets of a resource, skipping engine-internal names.
pub fn ref_targets(r: &resource::Resource) -> Vec<String> {
    r.ext_refs
        .iter()
        .filter(|x| !x.name.is_empty() && !x.name.starts_with('_'))
        .map(|x| x.compiled_path())
        .collect()
}

impl ModFile {
    pub fn from_vpk(v: &vpk::Vpk) -> vpk::Result<Vec<ModFile>> {
        v.entries
            .iter()
            .map(|e| Ok(ModFile { path: e.path.clone(), stored_crc: e.crc, inline: e.is_inline(), data: v.read(e)? }))
            .collect()
    }
}

const SOURCE_EXTS: &[&str] = &[
    "vmdl", "vmat", "vtex", "vpcf", "vsndevts", "vsnd", "vmap", "vanim", "vanmgrph", "vnmskel", "vnmclip",
    "vnmgraph", "vdata", "vjs", "vts", "vcss", "vxml", "vsvg", "vpost", "vphys", "vrr", "vmix", "vsndstck",
];
const LOOSE_MEDIA_EXTS: &[&str] = &["png", "tga", "jpg", "jpeg", "psd", "exr", "dds", "wav", "mp3", "ogg", "flac"];
const NOISE_NAMES: &[&str] = &["addoninfo.txt", "readme.txt", "readme.md", "thumbs.db", "desktop.ini", ".ds_store"];
const NOISE_EXTS: &[&str] = &[
    "md", "blend", "blend1", "fbx", "obj", "dmx", "spp", "zip", "7z", "bak", "log", "ini", "allow", "gitignore", "toml", "py",
    "ps1", "sh",
];
/// Files the toolchain writes that the game never loads from an addon.
const BUILD_ARTIFACT_EXTS: &[&str] = &["kv3", "vpk", "bin", "sqlite3", "soc", "vrman_c", "vrman"];

fn ext_of(path: &str) -> &str {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.rsplit_once('.').map(|(_, e)| e).unwrap_or("")
}

/// Raw text the engine does read directly (localization, keyvalue configs).
fn is_raw_text_the_engine_reads(path: &str) -> bool {
    (path.starts_with("resource/") && path.ends_with(".txt")) || path.starts_with("cfg/")
}

pub fn lint(input: &Input) -> Vec<Finding> {
    let mut out = Vec::new();

    // VPK-level shape.
    let name = input.vpk_name.to_ascii_lowercase();
    let ok_name = name.len() == "pak01_dir.vpk".len()
        && name.starts_with("pak")
        && name.ends_with("_dir.vpk")
        && name[3..5].chars().all(|c| c.is_ascii_digit())
        && &name[3..5] != "00";
    if !ok_name {
        out.push(finding("M050", None, format!("'{}' should be pakNN_dir.vpk with NN in 01..99", input.vpk_name)));
    }
    if input.files.is_empty() {
        out.push(finding("M052", None, "no files".into()));
        return out;
    }
    if let Some(f) = input.files.iter().find(|f| !f.inline) {
        out.push(finding("M051", Some(&f.path), "data stored outside the _dir.vpk".into()));
    }

    let mod_paths: HashSet<&str> = input.files.iter().map(|f| f.path.as_str()).collect();
    let mut referenced: HashSet<String> = HashSet::new();
    // For M033: the first mod file that names each debug-marked target.
    let mut debug_referrer: HashMap<String, String> = HashMap::new();

    for f in &input.files {
        let p = f.path.as_str();
        let ext = ext_of(p).to_ascii_lowercase();
        let file = p.rsplit('/').next().unwrap_or(p).to_ascii_lowercase();

        // a file the game never reads (M012) is not looked up either, so its case costs nothing
        let noise = NOISE_NAMES.contains(&file.as_str()) || NOISE_EXTS.contains(&ext.as_str()) || (ext == "txt" && !is_raw_text_the_engine_reads(p));
        if p.chars().any(|c| c.is_ascii_uppercase()) && !noise {
            out.push(finding("M001", Some(p), format!("rename to '{}'", p.to_ascii_lowercase())));
        }
        if p.contains('\\') || p.starts_with('/') || p.split('/').any(|s| s == ".." || s == ".") {
            out.push(finding("M002", Some(p), "path is not a clean forward-slash relative path".into()));
        }
        if vpk::crc32(&f.data) != f.stored_crc {
            out.push(finding("M020", Some(p), format!("stored {:08x}, actual {:08x}", f.stored_crc, vpk::crc32(&f.data))));
        }

        if p.starts_with("materials/default/") {
            out.push(match input.game.and_then(|g| g.read(p)) {
                Some(valve) if same_picture(&f.data, &valve) => Finding {
                    severity: Severity::Warn,
                    ..finding("M071", Some(p), "compiler byproduct with Valve's own picture (every mip decodes to the same pixels): it replaces the engine's default with the same thing, so nothing in the game changes; exclude materials/default/ from the package".into())
                },
                Some(_) => finding("M071", Some(p), "compiler byproduct whose pixels differ from Valve's default at this path: every material in the game that falls back to it draws the mod's picture; exclude materials/default/ from the package".into()),
                None if input.game.is_some() => Finding {
                    severity: Severity::Warn,
                    ..finding("M071", Some(p), "compiler byproduct at a path Valve does not ship: it replaces no default of the game's, only the mod's own materials read it; exclude materials/default/ from the package".into())
                },
                None => finding("M071", Some(p), "compiler byproduct: exclude materials/default/ from the package (no game to compare its picture with)".into()),
            });
            continue;
        }
        if BUILD_ARTIFACT_EXTS.contains(&ext.as_str()) {
            out.push(finding("M013", Some(p), format!("a .{ext} is a toolchain file, not game content: leave it out of the package")));
            continue;
        }
        if matches!(ext.as_str(), "vnmskel_c" | "vnmgraph_c" | "vdata_c") {
            if ext == "vdata_c" {
                if let Some(Ok(kv)) = resource::kv3_block(&f.data, b"DATA") {
                    if let Some(kind) = kv.get("generic_data_type").and_then(resource::kv3::Value::as_str).filter(|k| k.contains("image_list")) {
                        out.push(finding("M013", Some(p), format!("a {kind}: the CSDK compiles PSDs from it at build time; nothing loads it in game")));
                    }
                }
            }
            out.push(finding("M070", Some(p), "remove it: ship the model, never the skeleton, graph or data".into()));
            continue;
        }
        if SOURCE_EXTS.contains(&ext.as_str()) {
            out.push(finding("M010", Some(p), format!("ship the compiled '{p}_c' instead")));
            continue;
        }
        if LOOSE_MEDIA_EXTS.contains(&ext.as_str()) {
            out.push(finding("M011", Some(p), format!(".{ext} must be compiled into a resource")));
            continue;
        }
        if noise {
            out.push(finding("M012", Some(p), "remove from the VPK".into()));
            continue;
        }

        if ext.ends_with("_c") {
            match resource::parse(&f.data) {
                Err(e) => out.push(finding("M021", Some(p), e.to_string())),
                Ok(r) => {
                    let mut mine = ref_targets(&r);
                    let original = input.original_refs.get(p);
                    // Sound-event files name their .vsnd files as strings in the data, with no RERL entry.
                    let mut inherited_strings: Vec<String> = Vec::new();
                    if ext == "vsndevts_c" {
                        if let Some(Ok(kv)) = resource::kv3_block(&f.data, b"DATA") {
                            for t in deep::vsnd_refs(&kv) {
                                if !mine.contains(&t) {
                                    mine.push(t);
                                }
                            }
                        }
                        if let Some(Ok(kv)) = input.game.and_then(|g| g.read(p)).and_then(|d| resource::kv3_block(&d, b"DATA")) {
                            inherited_strings = deep::vsnd_refs(&kv);
                        }
                    }
                    for target in &mine {
                        let in_mod = mod_paths.contains(target.as_str());
                        let in_game = input.base.is_none_or(|b| b.contains(target));
                        let inherited = original.is_some_and(|o| o.contains(target)) || inherited_strings.contains(target);
                        if !in_mod && !in_game && !inherited {
                            out.push(finding("M030", Some(p), format!("needs '{target}'")));
                        }
                        referenced.insert(target.clone());
                        if debug_marker(target).is_some() {
                            debug_referrer.entry(target.clone()).or_insert_with(|| p.to_string());
                        }
                    }
                    if let Some(orig) = original {
                        for t in orig {
                            // Dropping a reference Valve itself couldn't resolve costs nothing.
                            let resolvable = input.base.is_none_or(|b| b.contains(t)) || mod_paths.contains(t.as_str());
                            if resolvable && plays(p, t) && !mine.contains(t) {
                                out.push(finding("M061", Some(p), format!("original referenced '{t}'")));
                            }
                        }
                    }
                }
            }
            if ext == "vmdl_c" && input.base.is_some_and(|b| b.contains(p)) {
                out.push(finding("M060", Some(p), "full model document replaced".into()));
            }
        }
    }

    deep_rules(input, &mut out);
    stale_leftovers(input, &mut out);
    dev_leftovers(input, &debug_referrer, &mut out);

    // Replacement-vs-addition and drift need a game baseline.
    if let Some(base) = input.base {
        // the heroes with a voice folder, read once and only for a mod that ships a line no event plays
        let mut voices: Option<Vec<String>> = None;
        for f in &input.files {
            let p = f.path.as_str();
            let ext = ext_of(p).to_ascii_lowercase();
            if !ext.ends_with("_c") {
                continue;
            }
            let replaces = base.contains(p);
            if !replaces && !referenced.contains(p) && !p.starts_with("materials/default/") {
                let others = p.starts_with("sounds/vo/").then(|| line_of_others(p, base, voices.get_or_insert_with(|| voice_folders(base)))).flatten();
                out.push(finding("M031", Some(p), match others {
                    Some((hero, q)) => format!("neither replaces a game file nor is referenced by the mod: a line Valve records for other heroes ({q}), not for {hero}, so no sound event plays it"),
                    None => "neither replaces a game file nor is referenced by the mod".into(),
                }));
            }
            if let Some(then) = input.built_against {
                match (then.get(p), base.get(p)) {
                    (Some(a), Some(b)) if a.crc != b.crc => out.push(finding(
                        "M040",
                        Some(p),
                        format!("original changed {} -> {} (crc {:08x} -> {:08x})", then.build, base.build, a.crc, b.crc),
                    )),
                    (Some(_), None) => out.push(finding("M041", Some(p), format!("existed in {}, gone in {}", then.build, base.build))),
                    _ => {}
                }
            }
        }
    }

    out.sort_by(|a, b| b.severity.cmp(&a.severity).then(a.code.cmp(b.code)).then(a.path.cmp(&b.path)));
    out
}

/// Whether two compiled textures draw the same picture: the same size and mip count, and every mip decodes to the same
/// pixels. The compiler's default copies differ from Valve's in their header (2,232 bytes against 2,184), so bytes
/// are not the question; what the sampler reads is.
fn same_picture(a: &[u8], b: &[u8]) -> bool {
    let (Some(ta), Some(tb)) = (resource::texture::info(a), resource::texture::info(b)) else { return false };
    if (ta.width, ta.height, ta.depth, ta.mips) != (tb.width, tb.height, tb.depth, tb.mips) {
        return false;
    }
    (0..ta.mips.max(1) as u32).all(|m| match (resource::pixels::decode(a, m), resource::pixels::decode(b, m)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    })
}

/// Whether `referrer` losing its reference to `target` stops something playing (M061). What a file draws with is
/// the art, not a dependency: a material, a texture or a vector image the original drew with and the replacement does
/// not is a restyle (it draws with its own), and so is the model a particle renders. The compiler names a texture
/// after its source and settings (`..._psd_3804f11c.vtex_c`), so Valve's own material recompiled unchanged refers to
/// other names; on three skins (6871) all 105 dropped references were of these kinds.
pub fn plays(referrer: &str, target: &str) -> bool {
    let ext = |p: &str| p.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let t = ext(target);
    !(matches!(t.as_str(), "vmat_c" | "vtex_c" | "vsvg_c") || t == "vmdl_c" && ext(referrer) == "vpcf_c")
}

/// Words that mark a path as debug content when they are a whole token of it. Whole tokens only: `testing`,
/// `contest`, `attempt` and `heroes_wip` (a Valve folder) must not match.
const DEBUG_MARKERS: &[&str] = &["probe", "probes", "debug", "dbg", "test", "tests", "tmp", "temp", "scratch", "dummy", "placeholder"];

/// The debug marker in a path, if one of its tokens (split at anything but a letter or digit; a trailing counter such
/// as `probe2` is ignored) is in `DEBUG_MARKERS`.
pub fn debug_marker(path: &str) -> Option<&'static str> {
    path.to_ascii_lowercase().split(|c: char| !c.is_ascii_alphanumeric()).find_map(|tok| {
        let bare = tok.trim_end_matches(|c: char| c.is_ascii_digit());
        let bare = if bare.len() >= 3 { bare } else { tok };
        DEBUG_MARKERS.iter().copied().find(|m| *m == bare)
    })
}

/// The folders under `sounds/vo/` the game has: one per hero voice, sorted.
fn voice_folders(base: &Baseline) -> Vec<String> {
    let mut v: Vec<String> = base.files.keys().filter_map(|p| p.strip_prefix("sounds/vo/")?.split_once('/').map(|(h, _)| h.to_string())).collect();
    v.sort();
    v.dedup();
    v
}

/// A voice line of one hero that the game has for other heroes and not for this one: the line is named as Valve
/// names it, so it reads as a line, but no sound event of this hero plays it (`fencer_ping_see_yamato` beside
/// `astro_ping_see_yamato`). The hero's folder and the first other hero's file with that line, in folder order.
fn line_of_others(p: &str, base: &Baseline, voices: &[String]) -> Option<(String, String)> {
    let rest = p.strip_prefix("sounds/vo/")?;
    let (hero, tail) = rest.split_once('/')?;
    let (sub, file) = match tail.rsplit_once('/') {
        Some((s, f)) => (format!("{s}/"), f),
        None => (String::new(), tail),
    };
    let line = file.strip_prefix(hero)?.strip_prefix('_')?;
    voices.iter().filter(|h| h.as_str() != hero).find_map(|h| {
        let q = format!("sounds/vo/{h}/{sub}{h}_{line}");
        base.contains(&q).then(|| (hero.to_string(), q.trim_end_matches("_c").to_string()))
    })
}

/// M033: new files (not Valve's own paths, whose names are Valve's) that read as debug content. A file M032 already
/// reports as a stale leftover is not reported twice.
fn dev_leftovers(input: &Input, referrers: &HashMap<String, String>, out: &mut Vec<Finding>) {
    let stale: HashSet<String> = out.iter().filter(|f| f.code == "M032").filter_map(|f| f.path.clone()).collect();
    for f in &input.files {
        let p = f.path.as_str();
        if !p.ends_with("_c") || p.starts_with("materials/default/") || stale.contains(p) || input.base.is_some_and(|b| b.contains(p)) {
            continue;
        }
        let Some(marker) = debug_marker(p) else { continue };
        // a material or texture something in the mod draws with is the art on screen, whatever the exporter named it
        // (`exporter_dummy_mat`); a referenced particle is still a probe spawned on every cast
        if (p.ends_with(".vmat_c") || p.ends_with(".vtex_c")) && referrers.contains_key(p) {
            continue;
        }
        let by = referrers.get(p).map_or("nothing in the mod references it".to_string(), |r| format!("referenced by {r}"));
        out.push(finding("M033", Some(p), format!("'{marker}' in the path marks debug content ({by}): leave it out of the release, or rename it if it is real art")));
    }
}

/// M032: with the addon's content tree known, every referenced mod file must still have a source (or be generated
/// by a referrer that does). Lint sees only compiled files, so the tree comes from the build (`--sources`).
fn stale_leftovers(input: &Input, out: &mut Vec<Finding>) {
    let Some(all) = input.game.and_then(|g| g.addon_sources()) else { return };
    let mut referrers: HashMap<String, Vec<String>> = HashMap::new();
    for f in &input.files {
        if let Ok(r) = resource::parse(&f.data) {
            let mut t = ref_targets(&r);
            if f.path.ends_with(".vsndevts_c") {
                if let Some(Ok(kv)) = resource::kv3_block(&f.data, b"DATA") {
                    t.extend(deep::vsnd_refs(&kv));
                }
            }
            for x in t {
                referrers.entry(x).or_default().push(f.path.clone());
            }
        }
    }
    for f in &input.files {
        let p = f.path.as_str();
        if !p.ends_with("_c") || p.starts_with("materials/default/") || sources::source_of(p, all).is_some() {
            continue;
        }
        let Some(by) = referrers.get(p) else { continue };
        if by.iter().any(|r| sources::generated_by(p, r) && sources::source_of(r, all).is_some()) {
            continue;
        }
        // Report where a live file reaches into the stale part: a leftover only reached through other leftovers
        // (the deleted material's own generated textures) goes away with them.
        let by: Vec<&String> = by.iter().filter(|r| sources::source_of(r, all).is_some()).collect();
        if by.is_empty() {
            continue;
        }
        let shadow = if input.base.is_some_and(|b| b.contains(p)) { "; it shadows the game's file of the same path" } else { "" };
        let mut by: Vec<String> = by.into_iter().cloned().collect();
        by.sort();
        by.dedup();
        out.push(finding(
            "M032",
            Some(p),
            format!("no source in the addon (deleted?), still referenced by {}{shadow}: restore the source or drop the reference", by.join(", ")),
        ));
    }
}

/// Rules that read inside compiled files and compare replacements with Valve's originals.
fn deep_rules(input: &Input, out: &mut Vec<Finding>) {
    let t_all = std::time::Instant::now();
    let fp = input.fair_play;
    use resource::kv3::Value;
    let by_path: HashMap<&str, &ModFile> = input.files.iter().map(|f| (f.path.as_str(), f)).collect();
    let replaces = |p: &str| input.base.is_some_and(|b| b.contains(p));
    let game_read = |p: &str| input.game.and_then(|g| g.read(p));
    let data_of = |bytes: &[u8]| resource::kv3_block(bytes, b"DATA").and_then(Result::ok);
    // Our view of a file: the mod's copy if it ships one, else the game's.
    let ours_kv = |p: &str| -> Option<Value> {
        match by_path.get(p) {
            Some(f) => data_of(&f.data),
            None => game_read(p).and_then(|d| data_of(&d)),
        }
    };
    let valve_kv = |p: &str| -> Option<Value> { game_read(p).and_then(|d| data_of(&d)) };
    let mut cookie_seen: HashSet<String> = HashSet::new();
    // every event the game's sound-event files define, read once and only when a replaced event changed its base
    let game_events: std::cell::OnceCell<HashMap<String, Value>> = std::cell::OnceCell::new();

    // The model rules (contract and the mesh family) are the expensive part: each model's verdict is
    // computed (or read from the findings cache) up front, the models side by side, and the file loop below
    // only reports them in file order.
    let game_id = format!("{}|{}", input.base.map_or("", |b| b.build.as_str()), input.game.map_or(String::new(), |g| g.identity()));
    let models: Vec<&ModFile> = input.files.iter().filter(|f| ext_of(&f.path).eq_ignore_ascii_case("vmdl_c")).collect();
    let model_results: HashMap<&str, Vec<(&'static str, String)>> = par::map(&models, |f| {
        (f.path.as_str(), model_findings(input, f, &by_path, &game_id))
    })
    .into_iter()
    .collect();

    for f in &input.files {
        let p = f.path.as_str();
        let ext = ext_of(p).to_ascii_lowercase();
        match ext.as_str() {
            "vpcf_c" => {
                let Some(kv) = data_of(&f.data) else {
                    if replaces(p) && input.game.is_some() {
                        out.push(finding("M144", Some(p), "the particle's data did not decode, so its energy and lights were not compared with Valve's (M081, M082)".into()));
                    }
                    continue;
                };
                if let Some(valve) = input.game.and_then(|g| g.particle_classes()) {
                    for m in deep::particle_class_problems(&kv, valve) {
                        out.push(finding("M083", Some(p), m));
                    }
                }
                // SKIN-FLAWS V20: our materials drawn as particle models
                for mat in mesh::particle_model_materials(&kv) {
                    let mp = deep::compiled(&mat);
                    if let Some(m) = by_path.get(mp.as_str()).and_then(|x| mesh::mat_info(&x.data)) {
                        if let Some((code, msg)) = mesh::rules::particle_prop(&mp, &m, p) {
                            out.push(finding(code, Some(&mp), msg));
                        }
                    }
                }
                let prof = deep::particle_profile(&kv);
                for (tex, field) in &prof.cookies {
                    let bytes = by_path.get(tex.as_str()).map(|t| t.data.clone()).or_else(|| game_read(tex));
                    if let Some(t) = bytes.as_deref().and_then(deep::tex_info) {
                        let problems = deep::cookie_problems(&t);
                        if !problems.is_empty() && cookie_seen.insert(tex.clone()) {
                            for m in problems {
                                out.push(finding("M072", Some(tex), format!("the {field} of {p}: {m}")));
                            }
                        }
                    }
                }
                if !replaces(p) {
                    continue;
                }
                let Some(vk) = valve_kv(p) else {
                    if input.game.is_some() {
                        out.push(finding("M144", Some(p), "Valve's original at this path did not decode, so the particle's energy and lights were not compared (M081, M082)".into()));
                    }
                    continue;
                };
                let vprof = deep::particle_profile(&vk);
                // Each light is judged in its own file; the tree only answers "were lights added under it".
                for m in deep::light_caps(&prof.lights, &vprof.lights, fp) {
                    out.push(finding("M082", Some(p), m));
                }
                if prof.lights.len() <= vprof.lights.len() {
                    let (ot, vt) = (deep::lights_in_tree(&kv, &ours_kv).len(), deep::lights_in_tree(&vk, &valve_kv).len());
                    if ot > vt {
                        out.push(finding("M082", Some(p), format!("{ot} light renderer(s) in the system and its children, Valve's tree has {vt}")));
                    }
                }
                for m in deep::particle_energy(&prof, &vprof, fp) {
                    out.push(finding("M081", Some(p), m));
                }
            }
            "vsndevts_c" if replaces(p) => {
                let (Some(kv), Some(vk)) = (data_of(&f.data), valve_kv(p)) else {
                    if input.game.is_some() {
                        let whose = if data_of(&f.data).is_none() { "its data" } else { "Valve's original at this path" };
                        out.push(finding("M144", Some(p), format!("{whose} did not decode, so the events were not compared with Valve's for hidden cues (M063)")));
                    }
                    continue;
                };
                let events = |n: &str| -> Option<Value> {
                    let all = game_events.get_or_init(|| {
                        let mut m: HashMap<String, Value> = HashMap::new();
                        let mut files: Vec<&String> = input.base.map(|b| b.files.keys().filter(|k| k.starts_with("soundevents/") && k.ends_with(".vsndevts_c")).collect()).unwrap_or_default();
                        files.sort();
                        for path in files {
                            if let Some(Value::Object(evs)) = valve_kv(path) {
                                for (k, v) in evs {
                                    m.entry(k).or_insert(v);
                                }
                            }
                        }
                        m
                    });
                    all.get(n).cloned()
                };
                let (less, more) = deep::sound_events(&kv, &vk, fp, &events);
                less.into_iter().for_each(|m| out.push(finding("M063", Some(p), m)));
                more.into_iter().for_each(|m| out.push(finding("M064", Some(p), m)));
            }
            "vsnd_c" if replaces(p) => {
                let ctrl = |d: &[u8]| resource::kv3_block(d, b"CTRL").and_then(Result::ok);
                let (Some(kv), Some(vk)) = (ctrl(&f.data), game_read(p).and_then(|d| ctrl(&d))) else {
                    if input.game.is_some() {
                        let whose = if ctrl(&f.data).is_none() { "its CTRL block" } else { "Valve's original's CTRL block" };
                        out.push(finding("M144", Some(p), format!("{whose} did not decode, so its length and loudness were not compared with Valve's (M063)")));
                    }
                    continue;
                };
                let (less, more) = deep::vsnd_compare(&kv, &vk, fp, p.starts_with("sounds/vo/"));
                less.into_iter().for_each(|m| out.push(finding("M063", Some(p), m)));
                more.into_iter().for_each(|m| out.push(finding("M064", Some(p), m)));
            }
            "vmat_c" => {
                for m in pbrcombo::problems(&f.data) {
                    out.push(finding("M143", Some(p), m));
                }
                // M149: a texture slot Valve's material has and ours lacks: the game still asks for it
                if let (Some(ours), Some(valve)) = (data_of(&f.data), replaces(p).then(|| valve_kv(p)).flatten()) {
                    let mine = deep::texture_params(&ours);
                    for t in deep::texture_params(&valve).into_iter().filter(|t| !mine.contains(t)) {
                        out.push(finding("M149", Some(p), format!("texture '{t}' dropped: Valve's material has it, so the game asks for it and draws its error texture there")));
                    }
                }
                // M148: an engine-default slot of Valve's material that ours turned into a different constant
                if let Some(valve) = replaces(p).then(|| game_read(p)).flatten() {
                    let read = |q: &str| by_path.get(q).map(|x| x.data.clone()).or_else(|| game_read(q));
                    for v in matconst::judge(&f.data, &valve, &read) {
                        match v {
                            matconst::Verdict::Changed(m) => out.push(finding("M148", Some(p), m)),
                            matconst::Verdict::Unchecked(m) => out.push(finding("M144", Some(p), m)),
                        }
                    }
                }
            }
            "vmdl_c" => {
                if let Some(found) = model_results.get(p) {
                    for (code, m) in found {
                        out.push(finding(code, Some(p), m.clone()));
                    }
                }
            }
            "vtex_c" => {
                let t = deep::tex_info(&f.data);
                if let Some(t) = &t {
                    if pow2_expected(p) && !t.pow2() {
                        out.push(finding("M073", Some(p), format!("{}x{} {}", t.width, t.height, t.format_name())));
                    }
                }
                if replaces(p) {
                    if let (Some(t), Some(v)) = (t, game_read(p).as_deref().and_then(deep::tex_info)) {
                        if let Some((code, m)) = mesh::rules::texture_parity(p, &t, &v) {
                            out.push(finding(code, Some(p), m));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(g) = input.game {
        if let Some(why) = g.users_unchecked() {
            let n = input.files.iter().filter(|f| replaces(&f.path)).count();
            if n > 0 {
                out.push(finding("M091", None, format!("the {n} file(s) this mod replaces were not checked for other users (M090): {why}")));
            }
        }
        for f in &input.files {
            let p = f.path.as_str();
            if !replaces(p) {
                continue;
            }
            let mut users = g.users(p);
            users.sort();
            users.dedup();
            if users.len() >= 2 {
                let shown: Vec<&str> = users.iter().take(8).map(String::as_str).collect();
                let more = if users.len() > 8 { format!(" and {} more", users.len() - 8) } else { String::new() };
                out.push(finding("M090", Some(p), format!("used by {} owners: {}{more}", users.len(), shown.join(", "))));
            }
        }
    }
    record("deep rules (all)", t_all);
}

/// The M062 contract check and the mesh family for one model, from the findings cache when its entry is
/// still true of this mod and game, else computed and stored.
fn model_findings(input: &Input, f: &ModFile, by_path: &HashMap<&str, &ModFile>, game_id: &str) -> Vec<(&'static str, String)> {
    use cache::Fingerprint;
    let p = f.path.as_str();
    let replaces = input.base.is_some_and(|b| b.contains(p));
    let game_read = |q: &str| input.game.and_then(|g| g.read(q));
    let fp = |q: &str| -> Fingerprint { by_path.get(q).map(|x| (x.stored_crc, x.data.len())) };
    let t = std::time::Instant::now();
    // In unit tests the process-wide cache directory is shared by tests running in parallel, so only the cache
    // test's own lint (named below) uses it: its hit, miss and entry counts are then its own.
    let use_cache = !cfg!(test) || input.vpk_name == CACHE_TEST_VPK;
    let key = cache::key(p, &f.data, input.deep, game_id, replaces, &input.fair_play.fingerprint());
    if let Some(e) = use_cache.then(|| cache::load(&key)).flatten() {
        let known: Option<Vec<(&'static str, String)>> =
            e.findings.iter().map(|(c, m)| RULES.iter().find(|r| r.code == c).map(|r| (r.code, m.clone()))).collect();
        if let (Some(found), true) = (known, e.valid_for(&fp)) {
            cache::note_hit();
            record("mesh family (cached)", t);
            return found;
        }
    }
    if use_cache {
        cache::note_miss();
    }
    let valve = if replaces { game_read(p) } else { None };
    let mut out: Vec<(&'static str, String)> = Vec::new();
    if let Some(valve) = &valve {
        match (deep::contract(&f.data), deep::contract(valve)) {
            (Some(a), Some(b)) => {
                for m in deep::contract_breaches(&a, &b) {
                    out.push(("M062", m));
                }
            }
            (a, _) => {
                let whose = if a.is_none() { "the model's" } else { "Valve's model's" };
                out.push(("M144", format!("{whose} contract (bones, attachments, hitboxes) could not be read, so the hitboxes were not compared with Valve's (M062)")));
            }
        }
    } else if replaces && input.game.is_some() {
        out.push(("M144", "Valve's model at this path could not be read: its contract and mesh were not compared (M062, M124)".into()));
    }
    // M141: the cloth judged as damped oscillators, against Valve's at the same path (cheap: the PHYS block and,
    // only for a ringing group whose numbers are Valve's own, the meshes bound to it)
    {
        let (oa, va) = (cloth::MeshArea::new(&f.data), valve.as_deref().map(cloth::MeshArea::new));
        let ours_area = |n: &[String]| oa.area(n);
        let valve_area = |n: &[String]| va.as_ref().and_then(|m| m.area(n));
        for m in cloth::judge(&f.data, valve.as_deref(), &ours_area, &valve_area) {
            out.push(("M141", m));
        }
        if let Some(m) = cloth::unreadable(&f.data) {
            out.push(("M100", m)); // unchecked is never clean
        }
    }
    // M142: Valve's distance fields (AO) that ours dropped
    if let Some(valve) = &valve {
        if let Some(m) = distfield::judge(&f.data, valve) {
            out.push(("M142", m));
        }
        if let Some(m) = distfield::unreadable(&f.data) {
            out.push(("M100", m));
        }
    }
    let deps: std::sync::Mutex<std::collections::BTreeMap<String, Fingerprint>> = Default::default();
    let ours_read = |q: &str| {
        if let Ok(mut d) = deps.lock() {
            d.insert(q.to_string(), fp(q));
        }
        by_path.get(q).map(|x| x.data.clone()).or_else(|| game_read(q))
    };
    let t = std::time::Instant::now();
    out.extend(mesh::check_model(&f.data, valve.as_deref(), &ours_read, &game_read, input.deep, input.fair_play));
    record(if input.deep { "mesh family (deep)" } else { "mesh family (light)" }, t);
    if use_cache {
        cache::store(
        &key,
        &cache::Entry { deps: deps.into_inner().unwrap_or_default(), findings: out.iter().map(|(c, m)| (c.to_string(), m.clone())).collect() },
        );
    }
    out
}

/// The only lint name that uses the cache under `cargo test` (see `model_findings`).
#[cfg(test)]
pub(crate) const CACHE_TEST_VPK: &str = "pak99_cache_test_dir.vpk";
#[cfg(not(test))]
const CACHE_TEST_VPK: &str = "";

/// Where Valve's own textures are all power-of-two. Build 6864: of 12,598 textures outside panorama/, 143
/// are not: 108 compiler morph atlases (`*_vmorf`), 24 voting cards, 5 minimap and 2 UI images, and 4
/// particle sheets. Under models/ there are none.
fn pow2_expected(p: &str) -> bool {
    !(p.starts_with("panorama/")
        || p.starts_with("materials/ui/")
        || p.starts_with("materials/voting/")
        || p.starts_with("materials/minimap/")
        || p.ends_with("_vmorf.vtex_c"))
}

#[cfg(test)]
mod tests;
