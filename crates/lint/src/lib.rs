//! Magician lint: deterministic checks that a mod VPK will behave in-game the way its author thinks.
//!
//! Every rule states what actually happens in the engine when it fires, not just what is "wrong".
//! Every rule has a fixture in `tests` built to trigger exactly that rule. A rule without a fixture
//! that proves it fires is a wish, not a check.

#![forbid(unsafe_code)]

pub mod allow;
pub mod attach;
pub mod baseline;
pub mod cache;
pub mod carry;
pub mod cloth;
pub mod corpus;
pub mod deep;
pub mod distfield;
pub mod gamedata;
pub mod layout;
pub mod matconst;
pub mod matparams;
pub mod pbrcombo;
pub mod physdrop;
pub mod policy;
pub mod reach;
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
/// witnessed in game (CHANGELOG, "judgement changed"). 3: the rules measured on Valve's 43 heroes (6871): M061, M063's
/// reach keys, M109 to M112, M135. 4: M062 judges a moved attachment by where it now sits against the body (6875).
/// 5: M062 judges a lost or reparented bone by what hangs on it and how the hero's animation moves it (6875).
/// 6: M062 reads the model's animation references and game data, and holds the game's camera attachments to Valve's
/// numbers (6877). 7: M062 judges a lost bone by who outside the model names it; M001, M030 (AnimGraph 1), M070,
/// M137 and M139 say a warning where the game still loads and plays (6880). 8: what the 6877 round left unverified,
/// seen in game (6880): M042 also says a layout's panel types the game no longer registers, and M065 is a note.
/// 9: M062 judges a lost attachment by who the game runs for the model and who names the attachment (6880).
pub const JUDGEMENT: u32 = 9;

/// The rule catalogue: one home for codes, names and in-game consequences.
pub const RULES: &[Rule] = &[
    Rule { code: "M001", name: "path-case", severity: Severity::Error,
        in_game: "Source 2 looks paths up in lowercase. A path with capitals that the game would look for (it replaces a game file, or the mod names it) is never found, so the original asset shows instead: an error. One nothing looks for (a leftover '- Copy', an exported folder) or one with a lowercase twin in the mod is never read whatever its case: a warning to remove it. So is a font under panorama/fonts/: Panorama finds fonts through fontconfig, which scans folders on disk (Valve ships all 82 of its fonts loose, none in a pak, and names them by family in stylesheets, never by file), so a font in a pak is not read at all (UNVERIFIED in game). Not said of a file the game never reads by kind (M012)." },
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
        in_game: "This resource depends on a file that exists in neither the mod nor the game build: missing material (pink checkerboard), error model, or missing effect. A file the game never reads (it replaces no game file, and no file the game reads names it) never asks for what it lacks: a warning that says so. A model's reference to an AnimGraph 1 graph (.vanmgrph) is a warning: Deadlock ships none and none of Valve's files names one (6880), and the hero animates from its AnimGraph 2 references, which M062 judges." },
    Rule { code: "M070", name: "ships-protected-file", severity: Severity::Error,
        in_game: "Skeletons (.vnmskel), anim graphs (.vnmgraph) and game data (.vdata) replace the hero's real files. A copy that still holds what Valve's file on this build has (the same bones in the same order, every control parameter the game sets, every entry) plays today and breaks at the next patch that changes Valve's file, which patch day and drift report: a warning that says so. A copy that already lacks some of it is out of date now (the hero animates wrongly, ignores what the game sets, or heroes and items go missing: UNVERIFIED in game): an error that names what. One at a path Valve does not ship replaces nothing: a warning. A model replacement keeps Valve's skeleton and graph." },
    Rule { code: "M071", name: "replaces-engine-default", severity: Severity::Error,
        in_game: "resourcecompiler emits its own copies of the engine's default textures (materials/default/*) next to your materials, and they differ from Valve's. Shipped with other pixels than Valve's, they replace the fallback texture for every material in the game that uses one (an error). A copy that decodes to Valve's own pixels, or sits at a path Valve does not ship, changes nothing in the game and is a warning (seen on three published mods, 6868)." },
    Rule { code: "M061", name: "dropped-dependency", severity: Severity::Warn,
        in_game: "The game's original of this file referenced something that plays (a particle, a sound or sound event, an anim graph or clip, a sound snapshot) and your replacement no longer does: that effect or sound silently stops playing. A material or texture the original drew with and yours does not is a restyle, not a loss: the replacement draws with its own (and the compiler names a texture after its settings, so the same art recompiled has other names)." },
    Rule { code: "M032", name: "stale-leftover", severity: Severity::Error,
        in_game: "A file the mod references has no source left in the addon: a compiled leftover of something deleted. It ships the old content (one release shipped five deleted effects this way, one a shock ring at 1.3x Valve's brightness) and no rebuild can ever change it." },
    Rule { code: "M033", name: "dev-leftover", severity: Severity::Warn,
        in_game: "A particle, material or file whose path marks it as debug content (probe, debug, test, tmp, scratch...) ships in the release. It is a test aid, not art: a probe child spawned on every cast (one release shipped one), a placeholder swatch, an effect nobody meant to show. If it is also a dangling reference, M030/M032 already report that as an error. A material or texture the mod draws with is not reported: it is the art on screen, whatever the exporter named it." },
    Rule { code: "M031", name: "orphan-new-file", severity: Severity::Warn,
        in_game: "The file doesn't replace anything in the game, and nothing in the mod references it, so it never loads. A reference counts from a compiled file's reference list or from Panorama's own text (a stylesheet's s2r:// url, a script's or layout's file://{images} or file://{resources} path, a layout's script). The model rules do not judge a model that never loads." },
    Rule { code: "M040", name: "drift-original-changed", severity: Severity::Warn,
        in_game: "Valve changed the original of a file you replace since you built against it. Your copy may now be missing fixes, attachments or new animations." },
    Rule { code: "M041", name: "drift-original-removed", severity: Severity::Error,
        in_game: "The file you replace no longer exists in this build (moved or renamed), so your replacement does nothing." },
    Rule { code: "M042", name: "layout-id-missing", severity: Severity::Error,
        in_game: "A replaced Panorama layout lacks an element id that Valve's layout at the same path has on this build: the mod's copy is of an older layout, and it masks Valve's new one. The game's code finds panels by id, so an id its client code names (read from the installed client library) is an error: the game stops at launch ('FATAL ERROR: Unable to find child', seen on 6880). An id the code does not name is a warning: Valve's scripts may still look it up. A layout that uses a panel type the game no longer registers (in neither Valve's layout nor the client or Panorama UI library) is an error too: the game cannot parse it, and a required layout such as hud.xml stops the game at launch ('FATAL ERROR: Unable to load layout file') and has Steam mark the install corrupt and verify it, which resets gameinfo.gi. Rebuild the layout from Valve's current file and apply the mod's change to it again." },
    Rule { code: "M050", name: "vpk-name", severity: Severity::Warn,
        in_game: "Addon VPKs are mounted as pakNN_dir.vpk (NN = 01..99, lowest wins). Other names are skipped by the addons search path and by mod managers' load-order logic." },
    Rule { code: "M051", name: "vpk-chunked", severity: Severity::Warn,
        in_game: "Data stored in chunk files (_000.vpk...) is easy to ship incomplete. Single-file VPKs are the safe form for addons." },
    Rule { code: "M052", name: "vpk-empty", severity: Severity::Error,
        in_game: "An empty VPK changes nothing." },
    Rule { code: "M013", name: "build-artifact", severity: Severity::Warn,
        in_game: "A build input or toolchain byproduct (a panorama image list, a nested VPK, a raw .kv3, a compiler cache) that the game never loads from an addon. It adds size, and a vdata among them is noted in the grade. magician build leaves these out." },
    Rule { code: "M062", name: "model-contract", severity: Severity::Error,
        in_game: "A replaced model lost or reparented a bone of Valve's, lost an attachment, moved one too far, or changed or added a hitbox. A lost bone is judged by who still names it, since a full skin brings its own mesh, cloth and ragdoll: an error when the model itself still names it (an attachment riding it, a hitbox, a physics part, a cloth node, a mesh's bone list) or the game's client code holds its name and the model has no attachment of that name, the message naming which; otherwise a warning that says what Valve's model hung on it. The hero's anim graphs name bones too (IK and follow targets, root motion), but they pose the animation skeleton, not the model, and Valve's own models lack bones their graphs name (6880). Who names each bone is the bone users index `magician index` writes; without it a lost bone stays an error that says so. The hero's animation poses a bone by name, so a reparented bone its animation skeleton has moves as Valve's whatever its parent, and one it lacks follows its new parent: a reparent is an error only where what the mod hangs on the bone lands or moves a fair-play pixel or more from where Valve's would. A lost attachment is judged by who names it: an error when something the game runs for the model names it (the model's own game data, the vdata of its unit's abilities or of items any hero carries, an animation event in a clip its graphs play, or the client code for a name all of Valve's playable heroes carry), since what is placed there goes to the origin; otherwise a warning that says who does name it (a particle editor preview, another unit's data, nothing). A name Valve's game data gives a role this model's game data gives elsewhere is not looked up on it. Who names each attachment is the attachment users index `magician index` writes. A changed hitbox changes what bullets hit: a fair-play break. A moved attachment on a body bone that still sits near the body is a warning that says how far it moved: a skin with another body shape moves where its effects attach, and that is cosmetic. Near is within 9.4% of the model's size (the square root of its surface), just past the furthest any attachment sits off the body on Valve's 43 heroes (6873 and 6875). Further off, its effect visibly spawns away from the hero: an error. So is a moved attachment held in the hero's own frame (on the root bone, as Valve's camera poses, aim and zipline points are), and one whose place could not be measured. The camera attachments the game names (the model's AttachmentCameraData, and the pivots the client code names in the hero's frame) are held to 1e-4 of Valve's numbers: Valve's copies are bit-identical, a Source 2 Viewer round trip leaves 6.1e-5 at most, and an older viewer's -89.972 for -90 centres the camera. The contract also covers what Deadlock reads from the model beyond its skeleton: a missing or other animation skeleton or AnimGraph 2 reference (the A-pose), a unit-status block that is missing or a list (a CSDK 12 compile: the health bar at the feet): errors. A tagged-sound set Valve's model plays from and ours lacks silences the footsteps enemies listen for, and camera settings other than Valve's change what a player sees around the hero: both fair play, never allowable. A health bar placed elsewhere than Valve's, or another game-data block Valve's model has and ours lacks, is a warning that says what falls back to the game's defaults." },
    Rule { code: "M063", name: "sound-cue-reduced", severity: Severity::Error,
        in_game: "A replaced sound event or file plays quieter, shorter, over less distance, to fewer players, or not at all compared with Valve's. Enemies rely on those cues: hiding them is a fair-play break. A changed base is read through the game's own base events: one Valve's base inherits from differs only by what lies between, and only a level or a reach there is a finding (louder is M064). A voice line (sounds/vo/), its file or its event, is not judged by its length: Valve's own voices fail that test against each other on 46% of shared lines (`magician corpus`)." },
    Rule { code: "M064", name: "sound-louder", severity: Severity::Warn,
        in_game: "A replaced sound plays louder, longer or carries further than Valve's. Not an advantage, but it pushes the hero over the game's mix and masks other players' cues." },
    Rule { code: "M065", name: "sound-format-parity", severity: Severity::Info,
        in_game: "A replaced sound file has another sample rate or channel count than Valve's file at the same path (most of Valve's are mono, 44,100 Hz), said with both lengths and the pitch and volume the game's event plays it at. Heard in game on 6880, it plays right: a 48,000 Hz file and a stereo file play at their own speed and pitch, a stereo file through a hero's 3D event is placed and as loud as a mono one, and played flat (`play`) it keeps its channels apart. So it is a note, not a fault: a replacement that sounds sped up or quiet comes from its own content or its event (M063, M064), not its format. CSDK 13 resamples a 48,000 Hz WAV to 44,100 Hz when it compiles one." },
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
    Rule { code: "M084", name: "particle-unreached", severity: Severity::Warn,
        in_game: "A replaced particle that nothing in the game names: no hero, ability or item data, no model, clip, map, layout or other particle references it, the client code does not name it, and nothing in the mod does. The game never plays it, so the change does not show (a recolour of an unused file, found today with `dumpparticlelist` in a match). Valve ships thousands of such particles, many of them effects of older versions of abilities: 3,758 of 18,043 on 6877. The list is built by `magician index` with the users index; a particle the code built a name for at run time would not be seen here, but none was found: on 6880 every particle `dumpparticlelist` showed live across seven heroes' abilities, melee and deaths (335 systems) is one the game names." },
    Rule { code: "M090", name: "shared-file", severity: Severity::Warn,
        in_game: "The game file this replaces is used by more than one hero, ability or item (Lash's Ground Strike icon is also Majestic Leap's). Replacing it changes all of them for everyone who installs the mod." },
    Rule { code: "M092", name: "other-hero-file", severity: Severity::Warn,
        in_game: "A mod that replaces one hero's model also replaces a file only another hero uses (read from the build's users index): everyone who installs it sees the other hero changed, though the mod is not about them. A template's sample image left in the addon changes another hero's icons this way. Files shared by several heroes are M090's." },
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
        in_game: "More of the model folds through itself at bind than any of Valve's heroes does (visible fold line in units of the model's size, the square root of its surface, against the most Valve's 43 playable heroes have, 6871): its layers flicker along the folds. One fold alone is not a finding: Valve's heroes have them too, and one cannot be told from Valve's construction." },
    Rule { code: "M110", name: "shell-intersection", severity: Severity::Warn,
        in_game: "More of the model passes through itself at bind than any of Valve's heroes does (a garment through the body, a spool through the bodice; visible crossing line in units of the model's size, the square root of its surface, against the most Valve's 43 playable heroes have, 6871): clips that show along the lines. One crossing alone is not a finding: every Valve hero is built from pieces sunk into each other, about 250 pairs a hero, with the same spread of crossing length as a skin's." },
    Rule { code: "M111", name: "floating-shell", severity: Severity::Warn,
        in_game: "A piece touches nothing it should rest on, or rests on a surface but is skinned to other bones: it hovers with a gap, or lifts off as soon as the hero moves (the arm band). A gap under one fair-play pixel (6 mm) is not one a camera shows, a piece that meets its substrate at the joint between their bones (a hand at its cuff, a finger on the next) turns about where it touches rather than lifting off, and a piece on a bone that carries nothing else (a ring on its ring bone, a pump on its pump) is a part the rig moves on purpose." },
    Rule { code: "M112", name: "coordinate-precision", severity: Severity::Warn,
        in_game: "A piece sits so far from the origin that float32 snaps its vertices by an eighth of a pixel or more, and one of its dimensions is held in fewer than eight of those steps: it cannot hold its shape and collapses into flickering slivers. Near the origin, where every hero is, the steps are far under a pixel and a flat card's few quanta of thickness are float32 rounding a plane." },
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
        in_game: "Our in-game silhouette is smaller than Valve's in a view: a target is harder to see than the original, a fair-play break. A target is what bullets can hit, a model Valve gives hitboxes (heroes, troopers, bosses, turrets, summons); a model with none (an ability's prop, a trooper's break-off pieces, a companion no one can shoot) is an effect, judged by its own measures." },
    Rule { code: "M125", name: "seam-crack", severity: Severity::Warn,
        in_game: "Boundary vertices of neighbouring pieces sit a hair apart without welding: a hairline crack shows the background along the seam." },
    Rule { code: "M126", name: "texture-without-uv", severity: Severity::Error,
        in_game: "A draw call uses a textured material but has no UVs: the whole piece samples one texel." },
    Rule { code: "M127", name: "unreferenced-vertices", severity: Severity::Info,
        in_game: "Vertices in LOD0's buffers that no triangle uses: memory with no picture." },
    Rule { code: "M128", name: "material-unused", severity: Severity::Info,
        in_game: "The default material group lists a material no draw call uses: dead weight in the model's references." },
    Rule { code: "M129", name: "hitbox-envelope", severity: Severity::Error,
        in_game: "Visible surface lies more than 18 units beyond every hitbox (capsule, sphere or box of the default set), or over 3.5% of it lies beyond 10 units, counting only what Valve's own model at this path does not already have in the same hitbox region: enemies aim at cloth that can't be hit and the silhouette reads as excessive, 'a bit much' (Magician's fair-play line, the defaults of a policy). Never allowable: an allowance does not waive it, and the grade reads the skin as freeform." },
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
        in_game: "A replaced texture shows worse where Valve draws it: smaller than Valve's (stretched up to Valve's size, it blurs), in a format that loses more than Valve's (block artefacts), or with fewer mips than Valve's where its size could hold them (drawn small, it shimmers). A larger image, or another format that keeps as much, shows nothing. A model's morph atlas (`_vmorf`) shipped with its own model is not judged: the compiler sizes it to that model's morphs, and only that model reads it." },
    Rule { code: "M136", name: "albedo-range", severity: Severity::Warn,
        in_game: "Over 10% of the new materials' area sits outside Valve's own albedo range for this model: the piece reads brighter or darker than everything Valve drew, flattening or blooming in the game's lighting." },
    Rule { code: "M137", name: "albedo-absolute", severity: Severity::Warn,
        in_game: "Over 5% of the new materials' area is absolute black (under 0.003 linear) or white (over 0.80): blacks lose every fold and read as holes, whites flatten and bloom (Valve's art guide: no absolute white or black). A look, not a fault: the game loads and draws the colours it is given, so it is a warning (6880)." },
    Rule { code: "M138", name: "alpha-coverage-drift", severity: Severity::Warn,
        in_game: "An alpha-tested material whose cut-out coverage changes by more than 30% on over 10% of its tiles at the mip the game samples at 8 m: lace turns solid or vanishes as it recedes, and sparkles in motion." },
    Rule { code: "M139", name: "alpha-coverage-gameplay", severity: Severity::Warn,
        in_game: "The same coverage drift already at the gameplay distance (4 m): the lace, hair or lashes visibly change density on screen in normal play. A look, not a fault: the game draws every mip it is given, so it is a warning (6880). Author the alpha with coverage kept across mips, or a mip chain that holds it." },
    Rule { code: "M140", name: "hitbox-envelope-taste", severity: Severity::Warn,
        in_game: "Visible surface lies more than 10 units beyond every hitbox, beyond what Valve's own model has there and short of M129's line: enemies aim at cloth that can't be hit. Tasteful overflow is allowed: accept it with a written reason in magician.allow if the silhouette piece earns it." },
    Rule { code: "M141", name: "cloth-flutter", severity: Severity::Warn,
        in_game: "Crunchy fabric: a cloth group that rings for many swings at or above the body's own rhythm (the compiled goal, damping and gravity judged as a damped oscillator at the solver's step) flutters like paper-thin silk the moment the hero moves, however heavy the mesh looks, and with no collision radius or shape on its layers it passes through the body. A group with no damper at all (vertex attraction 0, what a goal-0 joint compiles to) keeps the whole of its swing every step and never settles, however slowly it sways. Valve's own numbers are not blamed unless the mod hangs a bigger mesh on them." },
    Rule { code: "M142", name: "distance-field-dropped", severity: Severity::Warn,
        in_game: "Valve's model carries distance fields (one voxel grid per bone, the ambient occlusion the body casts on itself and the world) and ours has none or fewer: the model loses that occlusion and its contact shadowing, silently (a model built from a Source 2 Viewer decompile always drops them). Fair-play neutral: accept it with a reason in magician.allow." },
    Rule { code: "M160", name: "physics-dropped", severity: Severity::Warn,
        in_game: "Valve's model simulates cloth and ours has none, or Valve's carries physics parts (a hero's ragdoll, a prop's body) that ours lacks: the pieces Valve's cloth swings hang stiff, and when the model dies, falls or breaks the lost parts are not simulated: they keep their last pose (seen on 6880, a hero's gun left hanging in the air after his death). A model built from a Source 2 Viewer decompile has no cloth, and its ragdoll joints are lost the same way; the compile says nothing. A skin's own cloth with other node counts is its design (M141 judges how it moves); only cloth gone altogether is said. Fair-play neutral: accept it with a reason in magician.allow." },
    Rule { code: "M143", name: "pbr-combo-invalid", severity: Severity::Error,
        in_game: "A pbr material whose static combo breaks the shader's own rules (F_SHEEN with F_DETAIL, F_GLASS, F_CLOAK, F_TRANSLUCENT, F_UNLIT; F_DETAIL with F_UNLIT; the status proxy without NPR, unlit or self-illum or with F_CLOAK; front-face normals without alpha test and backfaces): the compiler exits 0 and keeps the flags but silently drops the inputs of one feature (F_SHEEN + F_DETAIL loses g_tDetail), and the game has no such shader variant, so the material renders without it." },
    Rule { code: "M144", name: "fair-play-unchecked", severity: Severity::Warn,
        in_game: "A fair-play comparison could not run on this file: its data, or Valve's original at the same path, did not decode (a new format after a patch, a damaged file), or the model gave nothing to measure. Whether it hides a cue, shrinks the silhouette or overflows the hitboxes is unknown, and unknown is never a pass: the grade's fair play reads fair play not checked until it runs." },
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
    Rule { code: "M150", name: "material-param-unread", severity: Severity::Warn,
        in_game: "A material sets a feature or parameter that none of Valve's materials of the same shader sets on this build. The compiler keeps any name it is given, a typo or another shader's parameter included, and exits 0, so the setting is in the compiled file and does nothing in game. A rim-light strength is the scene's (the game's light rig), not the material's. Valve's materials are the evidence of what the shader reads: a name only one Valve material sets still counts as read." },
    Rule { code: "M151", name: "material-shader-changed", severity: Severity::Warn,
        in_game: "A replaced material draws with another shader than Valve's material at the same path. A hero's look (its NPR lighting, outline and status-effect overlay) is features of pbr.vfx, so a hero material on any other shader loses them, and the inputs it was given mean something else or nothing. The CSDK's Material Editor starts a new material on environment_texture_set (the DefaultShader in its gameinfo), which is how a skin ends up there." },
    Rule { code: "M152", name: "material-flag-fair-play", severity: Severity::Error,
        in_game: "A new material on a target (a model Valve gives hitboxes) switches on a feature Divine Skins' material reference marks Avoid for hero skins, and none of Valve's materials on that model has it: F_TRANSLUCENT, F_ADDITIVE_BLEND or F_ADVANCED_TRANSLUCENCY (see-through), F_SELF_ILLUM (glows), F_UNLIT (ignores light and shadow), F_OVERRIDE_BLOOM_AMOUNT (other bloom), F_OVERRIDE_NPR_OUTLINE (another outline) or F_DISABLE_NPR_OUTLINE (no outline). The hero reads differently in a fight. Fair play, never allowable: the skin loads and plays and is labelled freeform, never broken. Valve's own heroes use every one of these somewhere (on 6880, 101 of its 772 hero materials are translucent and 346 glow), so only a feature Valve's model at the same path never uses is said." },
    Rule { code: "M153", name: "draw-error-material", severity: Severity::Error,
        in_game: "A draw call at LOD0 has no material, the engine's error material, or a bare name with no folder that is no material in the mod or the game (a modelling tool's slot exported without one, a .001 duplicate): the game draws its error material there, the solid red or checkered piece the community pages report (UNVERIFIED here). None of Valve's hero models has one (6880); Valve's error and dev materials sit on effect models particles draw with their own material, and a draw Valve's model at the same path makes is not said." },
    Rule { code: "M170", name: "compiled-by", severity: Severity::Info,
        in_game: "Which compiler built the mod's files: a compiled file's edit info names each compiler with a version it raises when what it writes changes, and the game's own files of each kind carry one version on a build (6880: models 3, clips 21, graphs 27, animation skeletons 14, particles 2, layouts 16, styles 11, sound events 11). Files of another version were built by an older compiler or kit, named where its effect is known (CSDK 12 compiles AnimGraph 2 files in an outdated form, the community pages say). A note, not a fault: whether the game reads an older version as it was made is UNVERIFIED." },
    Rule { code: "M060", name: "model-document-replaced", severity: Severity::Info,
        in_game: "Replacing a .vmdl_c replaces the WHOLE model document: hitboxes, attachments, cloth, LODs and anim events come from your file. Run the contract check against the original." },
];

pub fn rule(code: &str) -> &'static Rule {
    RULES.iter().find(|r| r.code == code).expect("unknown rule code")
}

/// Where a player meets what a rule finds, in the order a report lists them: on the hero, in its effects and sound,
/// on other heroes, in the frame rate, in whether the game loads it at all, and in the pak alone (nothing a player
/// sees). Fair play is judged apart and is not grouped here.
pub const SEEN: &[&str] = &["on the hero", "effects, sound", "other heroes", "frame cost", "loading", "the pak"];

/// The `SEEN` group of a rule code.
pub fn seen(code: &str) -> &'static str {
    let n: u32 = code.strip_prefix('M').and_then(|x| x.parse().ok()).unwrap_or(0);
    match n {
        33 | 61..=65 | 72 | 81..=84 | 134 => "effects, sound",
        90..=92 => "other heroes",
        120 | 122 | 123 | 127 | 128 | 146 => "frame cost",
        1 | 2 | 10 | 20 | 21 | 30 | 32 | 40..=42 | 50..=52 | 70 | 71 | 170 => "loading",
        11..=13 | 31 | 60 | 100 | 144 => "the pak",
        _ => "on the hero",
    }
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
    /// The names Valve's materials set per shader on the build (cached per build). `None` when unavailable: then
    /// M150 can't run.
    fn material_params(&self) -> Option<&matparams::MaterialParams> {
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
    /// The names the game's client code holds as strings (every NUL-terminated run of printable ASCII in the client
    /// library): which attachments, panel ids and particles the game asks for by name. `None` when it can't be read:
    /// then only the game's data says which those are.
    fn code_names(&self) -> Option<&HashSet<String>> {
        None
    }
    /// The strings of the engine's Panorama UI library (`panoramauiclient`): the generic panel types it registers
    /// (`TabButton`, `DropDown`), which a layout may use beside the client's own. Empty when it can't be read.
    fn ui_names(&self) -> Option<&HashSet<String>> {
        None
    }
    /// The particles nothing in the game names: no compiled file references them, no vdata, layout or map names them,
    /// and the client code does not (`magician index` writes them beside the users index). `None` when there is no such
    /// list for the build: then M084 can't run.
    fn unreached_particles(&self) -> Option<&HashSet<String>> {
        None
    }
    /// Who outside a model names each bone (`magician index` writes it beside the users index). `None` when there is no
    /// such file for the build: then who names a lost bone is unknown, and M062 keeps it an error that says so.
    fn bone_users(&self) -> Option<&carry::BoneUsers> {
        None
    }
    /// Who outside a model names each attachment (`magician index` writes it beside the users index). `None` when
    /// there is no such file for the build: then who names a lost attachment is unknown, and M062 keeps it an error
    /// that says so.
    fn attachment_users(&self) -> Option<&attach::AttachmentUsers> {
        None
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
    /// Every file of the pak, in its directory's order, read in the order the bytes lie on disk.
    pub fn from_vpk(v: &vpk::Vpk) -> vpk::Result<Vec<ModFile>> {
        let entries: Vec<&vpk::Entry> = v.entries.iter().collect();
        entries
            .iter()
            .zip(v.read_many(&entries))
            .map(|(e, data)| Ok(ModFile { path: e.path.clone(), stored_crc: e.crc, inline: e.is_inline(), data: data? }))
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

/// M070: a shipped skeleton, anim graph or vdata, judged against Valve's file at the same path on this build. A copy that
/// still holds what Valve's current file has (the same bones in the same order; every control parameter the game's
/// code sets; every entry) plays today, and breaks at the next patch that changes Valve's file, which patch day and
/// drift catch (M040): a warning that says so. One that lacks some of it is already out of date: an error that says
/// what. A file at a path Valve does not ship replaces nothing. With no game to compare, it stays an error.
fn protected_file(ext: &str, ours: &[u8], at_game_path: Option<bool>, valve: Option<&[u8]>) -> (Severity, String) {
    use resource::kv3::Value;
    let what = match ext {
        "vnmskel_c" => "animation skeleton",
        "vnmgraph_c" => "anim graph",
        _ => "game data file",
    };
    if at_game_path == Some(false) {
        return (Severity::Warn, format!("a {what} at a path Valve does not ship: it replaces none of the game's files, and the game reads it only if one of the mod's own files names it, so the player sees nothing of it otherwise; leave it out unless the mod uses it"));
    }
    let data = |d: &[u8]| resource::kv3_block(d, b"DATA").and_then(Result::ok);
    let (Some(o), Some(v)) = (data(ours), valve.and_then(data)) else {
        return (Severity::Error, format!("replaces the hero's {what}, and it could not be compared with Valve's on this build: remove it and ship the model only, so the game keeps Valve's"));
    };
    let strs = |x: &Value, k: &str| -> Vec<String> { x.get(k).map(Value::as_array).unwrap_or(&[]).iter().filter_map(Value::as_str).map(str::to_string).collect() };
    let shown = |v: &[String]| {
        let s: Vec<&str> = v.iter().take(6).map(String::as_str).collect();
        if v.len() > 6 { format!("{} and {} more", s.join(", "), v.len() - 6) } else { s.join(", ") }
    };
    let lacking: Vec<String> = match ext {
        "vnmskel_c" => {
            let (a, b) = (strs(&o, "m_boneIDs"), strs(&v, "m_boneIDs"));
            if a != b {
                let missing: Vec<String> = b.iter().filter(|x| !a.contains(x)).cloned().collect();
                let detail = if missing.is_empty() { "the same bones in another order or with others added".to_string() } else { format!("{} of Valve's bones missing ({})", missing.len(), shown(&missing)) };
                return (Severity::Error, format!("replaces the hero's animation skeleton with one unlike Valve's on this build ({detail}): Valve's clips and graphs are built against Valve's bones, so the hero animates wrongly or not at all (UNVERIFIED); remove it and keep Valve's skeleton"));
            }
            Vec::new()
        }
        "vnmgraph_c" => {
            let mine = strs(&o, "m_controlParameterIDs");
            strs(&v, "m_controlParameterIDs").into_iter().filter(|x| !mine.iter().any(|m| m.eq_ignore_ascii_case(x))).collect()
        }
        _ => {
            // a key that starts with '_' (`_include`) is the file's own bookkeeping, not an entry the game reads
            let mine: HashSet<&str> = o.members().iter().map(|(k, _)| k.as_str()).collect();
            v.members().iter().map(|(k, _)| k.clone()).filter(|k| !k.starts_with('_') && !mine.contains(k.as_str())).collect()
        }
    };
    if !lacking.is_empty() {
        let (kind, effect) = if ext == "vnmgraph_c" {
            ("control parameters the game sets", "the hero no longer reacts to them (UNVERIFIED)")
        } else {
            ("entries", "while the mod is installed the game finds none of them: heroes, abilities, items or settings Valve has added go missing, the locked heroes and 'Error parsing KV3 data' the community pages report for an old heroes.vdata (UNVERIFIED)")
        };
        return (Severity::Error, format!("replaces the hero's {what} with a copy of an older build: it lacks {} {kind} that Valve's file on this build has ({}), so {effect}; remove it, or rebuild it from Valve's current file", lacking.len(), shown(&lacking)));
    }
    (Severity::Warn, format!("replaces the hero's {what} with a copy that holds everything Valve's file on this build has, so it plays today; the next patch that changes Valve's file leaves this copy behind and breaks the hero, which patchday and drift report (M040) when it happens; remove it and ship the model only to be safe from that"))
}

/// What a kit is known to do to files of one kind when an older compiler made them, as the community's pages say it.
fn kit_cause(ext: &str) -> &'static str {
    match ext {
        "vnmclip_c" | "vnmgraph_c" | "vnmskel_c" => " (a known kit cause: CSDK 12, on CS2's tools, compiles AnimGraph 2 files in an outdated form, the community pages say)",
        "vanmgrph_c" => " (an AnimGraph 1 graph, which CSDK 12's bin compiles; Deadlock reads AnimGraph 2)",
        _ => "",
    }
}

/// M170: which compiler built each kind of file, from the compiler versions in its edit info (`resource::compilers`),
/// against those of Valve's own file of that kind on this build (Valve's file at the same path when the mod replaces
/// one, else the first of that kind in the build). One note per kind and version that differs, with how many files
/// and one of them; a known kit cause is named.
fn compiled_by(input: &Input, out: &mut Vec<Finding>) {
    let (Some(base), Some(game)) = (input.base, input.game) else { return };
    let mut sample: HashMap<String, Option<String>> = HashMap::new();
    let mut valve_of = |p: &str, ext: &str| -> Vec<(String, i64)> {
        let path = if base.contains(p) {
            Some(p.to_string())
        } else {
            sample.entry(ext.to_string()).or_insert_with(|| base.files.keys().filter(|k| k.ends_with(&format!(".{ext}"))).min().cloned()).clone()
        };
        path.and_then(|q| game.read(&q)).map(|d| resource::compilers(&d)).unwrap_or_default()
    };
    // (kind, compiler, ours, Valve's) -> (files, first file)
    let mut seen: std::collections::BTreeMap<(String, String, i64, i64), (usize, String)> = std::collections::BTreeMap::new();
    for f in &input.files {
        let ext = ext_of(&f.path).to_ascii_lowercase();
        if !ext.ends_with("_c") {
            continue;
        }
        let ours = resource::compilers(&f.data);
        if ours.is_empty() {
            continue;
        }
        let valve = valve_of(&f.path, &ext);
        for (name, v) in &ours {
            let Some(theirs) = valve.iter().find(|(n, _)| n == name).map(|x| x.1) else { continue };
            if *v != theirs {
                let e = seen.entry((ext.clone(), name.clone(), *v, theirs)).or_insert((0, f.path.clone()));
                e.0 += 1;
            }
        }
    }
    for ((ext, name, v, theirs), (n, first)) in seen {
        let age = if v < theirs { "an older" } else { "another" };
        let files = if n == 1 { format!("1 .{ext} ({first})") } else { format!("{n} .{ext} files (one is {first})") };
        out.push(finding("M170", None, format!("{files} carry {name} {v} where the game's own .{ext} files on this build carry {theirs}: {age} compiler or kit built them{}; whether the game reads them as they were made is UNVERIFIED; recompile them with a kit whose compiler matches this build (CSDK 13 with Deadlock's own compiler dlls, the bin Magician calls win64_nm, wrote this build's model, clip and particle versions on 6880)", kit_cause(&ext))));
    }
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
    // what each file of the mod names, for which of them the game reaches (M030 on a file it never reads)
    let mut names: HashMap<&str, Vec<String>> = HashMap::new();
    // For M033: the first mod file that names each debug-marked target.
    let mut debug_referrer: HashMap<String, String> = HashMap::new();
    // M001: paths with capitals, judged after the loop
    let mut case_paths: Vec<&str> = Vec::new();

    for f in &input.files {
        let p = f.path.as_str();
        let ext = ext_of(p).to_ascii_lowercase();
        let file = p.rsplit('/').next().unwrap_or(p).to_ascii_lowercase();

        // a file the game never reads (M012) is not looked up either, so its case costs nothing
        let noise = NOISE_NAMES.contains(&file.as_str()) || NOISE_EXTS.contains(&ext.as_str()) || (ext == "txt" && !is_raw_text_the_engine_reads(p));
        if p.chars().any(|c| c.is_ascii_uppercase()) && !noise {
            // judged once every reference is known: a path nothing looks up costs nothing for its case
            case_paths.push(p);
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
            let at_game_path = input.base.map(|b| b.contains(p));
            let (severity, message) = protected_file(&ext, &f.data, at_game_path, input.game.and_then(|g| g.read(p)).as_deref());
            out.push(Finding { severity, ..finding("M070", Some(p), message) });
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
                    // Panorama names what it draws in its own text: a stylesheet's `url("s2r://...")` and a script's
                    // or layout's `file://{images}/...`, which the compiler does not list as references.
                    if matches!(ext.as_str(), "vcss_c" | "vxml_c" | "vjs_c" | "vts_c") {
                        let named = panorama_refs(&f.data);
                        names.entry(p).or_default().extend(named.iter().cloned());
                        referenced.extend(named);
                    }
                    // A layout compiled into a `LaCo` block keeps its tree LZ4-compressed, so its scripts and styles
                    // (`panorama/scripts/x.vjs_c` under SCRIPTS) are read from the decoded tree, not the raw bytes.
                    if ext == "vxml_c" {
                        if let Some(Ok(tree)) = resource::kv3_block(&f.data, b"LaCo") {
                            for s in tree.strings() {
                                let named = panorama_refs(s.as_bytes());
                                names.entry(p).or_default().extend(named.iter().cloned());
                                referenced.extend(named);
                            }
                        }
                    }
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
                    // a model's AnimGraph 1 include models (`m_refAnimIncludeModels`), which lend it their sequences
                    let includes: Vec<String> = if ext == "vmdl_c" { anim_include_models(&f.data) } else { Vec::new() };
                    for target in &mine {
                        let in_mod = mod_paths.contains(target.as_str());
                        let in_game = input.base.is_none_or(|b| b.contains(target));
                        let inherited = original.is_some_and(|o| o.contains(target)) || inherited_strings.contains(target);
                        if !in_mod && !in_game && !inherited {
                            if includes.contains(target) {
                                out.push(Finding {
                                    severity: Severity::Warn,
                                    ..finding("M030", Some(p), format!("needs '{target}', an AnimGraph 1 include model (m_refAnimIncludeModels) that lends the model its sequences: no model of Valve's names one (6880), and the hero animates from its AnimGraph 2 references, which M062 judges, so the player sees nothing of this; the game may log the missing file (UNVERIFIED); to be rid of it, take the include model out of the model document and recompile"))
                                });
                            } else if target.ends_with(".vanmgrph_c") {
                                // AnimGraph 1: no build ships one and none of Valve's files names one (6880); the hero
                                // animates from the model's AnimGraph 2 references, which M062 judges
                                out.push(Finding {
                                    severity: Severity::Warn,
                                    ..finding("M030", Some(p), format!("needs '{target}', an AnimGraph 1 graph: Deadlock ships none and none of Valve's files names one, so the hero animates from the model's AnimGraph 2 references (M062 judges those) and the player sees nothing of this; the game may log the missing file (UNVERIFIED); to be rid of it, take the AnimGraph 1 graph out of the model document and recompile"))
                                });
                            } else {
                                out.push(finding("M030", Some(p), format!("needs '{target}'")));
                            }
                        }
                        referenced.insert(target.clone());
                        names.entry(p).or_default().push(target.clone());
                        if debug_marker(target).is_some() {
                            debug_referrer.entry(target.clone()).or_insert_with(|| p.to_string());
                        }
                    }
                    if let Some(orig) = original {
                        // a model's own animation skeleton and graphs are M062's (`gamedata::judge`), which reads them
                        // from Valve's model whenever the game is there
                        let contract_judges = |t: &str| ext == "vmdl_c" && input.game.is_some() && (t.ends_with(".vnmgraph_c") || t.ends_with(".vnmskel_c"));
                        for t in orig.iter().filter(|t| !contract_judges(t)) {
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

    // M030 on a file the game never reads: one that replaces no game file and that no file the game reads names, at any
    // remove (a material only an unused model draws, an older copy of an event file). The game never opens it, so the
    // file it lacks is never asked for. Valve's own build has such references too: Infernus's unused arm material names
    // three textures 6880 does not ship.
    if input.base.is_some() {
        let mut read: HashSet<&str> = input.files.iter().map(|f| f.path.as_str()).filter(|p| input.base.is_some_and(|b| b.contains(p)) || p.starts_with("materials/default/")).collect();
        let mut todo: Vec<&str> = read.iter().copied().collect();
        while let Some(p) = todo.pop() {
            for t in names.get(p).map(Vec::as_slice).unwrap_or(&[]) {
                if let Some(q) = mod_paths.get(t.as_str()) {
                    if read.insert(q) {
                        todo.push(q);
                    }
                }
            }
        }
        for x in out.iter_mut().filter(|x| x.code == "M030" && x.severity == Severity::Error) {
            if x.path.as_deref().is_some_and(|p| !read.contains(p)) {
                x.severity = Severity::Warn;
                x.message.push_str(&format!("; {NEVER_READ}"));
            }
        }
    }

    // M001: the game looks a path up in lowercase. A file with capitals the game would look for (it replaces a game
    // file, or the mod names it) is never found: an error. One nothing looks for (a leftover copy, a folder the export
    // left behind) is never read whatever its case, and a lowercase twin in the mod is what a lookup finds: a warning.
    for p in case_paths {
        let l = p.to_ascii_lowercase();
        let twin = mod_paths.contains(l.as_str());
        let wanted = input.base.is_none_or(|b| b.contains(&l)) || referenced.contains(&l);
        // Panorama finds its fonts through fontconfig, which scans the folders `panorama/fonts/fonts.conf` lists on
        // disk (`../../citadel/panorama/fonts`, the system's font folders) for the file names its `fontpattern`s allow;
        // stylesheets name a font by family, never by file. Valve ships every font loose beside the paks (82 on 6880,
        // none in a pak) and none of its stylesheets or layouts names a font file. A font in a pak is not on disk, so
        // fontconfig never sees it, whatever its case (UNVERIFIED in game).
        if l.starts_with("panorama/fonts/") && !twin && !referenced.contains(&l) {
            out.push(Finding { severity: Severity::Warn, ..finding("M001", Some(p), format!("{FONT_IN_PAK}; renaming it to '{l}' changes nothing")) });
            continue;
        }
        if twin {
            out.push(Finding { severity: Severity::Warn, ..finding("M001", Some(p), format!("a copy of '{l}', which the mod also ships: the game looks paths up in lowercase and finds that one, so this copy is never read and the player sees nothing of it; remove it")) });
        } else if !wanted {
            out.push(Finding { severity: Severity::Warn, ..finding("M001", Some(p), format!("nothing in the game or the mod names '{l}', so the game never looks this file up and its case changes nothing the player sees; a leftover (a '- Copy', an exported folder): remove it, or rename it to '{l}' if something should use it")) });
        } else {
            out.push(finding("M001", Some(p), format!("rename to '{l}': the game looks it up in lowercase, never finds this file, and shows the original asset or nothing instead")));
        }
    }

    compiled_by(input, &mut out);
    deep_rules(input, &referenced, &mut out);
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

/// The models a compiled model includes for their AnimGraph 1 sequences (`m_refAnimIncludeModels`), as compiled paths.
fn anim_include_models(model: &[u8]) -> Vec<String> {
    let Some(Ok(d)) = resource::kv3_block(model, b"DATA") else { return Vec::new() };
    d.get("m_refAnimIncludeModels").map(resource::kv3::Value::as_array).unwrap_or(&[]).iter().filter_map(resource::kv3::Value::as_str).map(|p| {
        let p = p.to_ascii_lowercase().replace('\\', "/");
        if p.ends_with("_c") { p } else { format!("{p}_c") }
    }).collect()
}

/// Said of a font file shipped in a pak (M001 as a warning).
pub const FONT_IN_PAK: &str = "a font in a pak, which Panorama does not read: it finds fonts through fontconfig, which scans the font folders its fonts.conf lists on disk (Valve ships every font loose in game/citadel/panorama/fonts, none in a pak) and names them by family in stylesheets, never by file, so a stylesheet asking for this font's family gets a fallback font (UNVERIFIED in game)";

/// Said of a dangling reference in a file the game never reads (M030 as a warning).
pub const NEVER_READ: &str = "but the game never reads this file: it replaces no game file, and no file the game reads names it, so the missing file is never asked for and the player sees nothing of it; remove the file, or name it from what the game reads";

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

/// The files a Panorama stylesheet, layout or script names in its text, as compiled paths: `s2r://<path>` is the
/// path itself, `file://{images}/<x>.<ext>` an image under `panorama/images/` (`.svg` compiles to `.vsvg_c`, any other
/// image to `_<ext>.vtex_c`), `file://{resources}/<x>` a file under `panorama/`, and a bare `panorama/<x>.vts` as a
/// compiled layout lists its scripts.
pub fn panorama_refs(data: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(data);
    let mut out = Vec::new();
    let end = |s: &str| s.find(['"', '\'', ')', '(', ' ', ';', ',', '\n', '\r', '\t', '\0', '<', '>']).unwrap_or(s.len());
    for (key, root) in [("s2r://", ""), ("file://{images}/", "panorama/images/"), ("file://{resources}/", "panorama/"), ("panorama/", "panorama/")] {
        for (i, _) in text.match_indices(key) {
            let rest = &text[i + key.len()..];
            let raw = rest[..end(rest)].to_ascii_lowercase();
            if raw.is_empty() {
                continue;
            }
            let path = format!("{root}{raw}");
            let compiled = match path.rsplit_once('.') {
                _ if path.ends_with("_c") => path.clone(),
                Some((stem, "svg")) if !root.is_empty() => format!("{stem}.vsvg_c"),
                Some((stem, e @ ("png" | "jpg" | "jpeg" | "tga" | "psd"))) if !root.is_empty() => format!("{stem}_{e}.vtex_c"),
                Some((stem, "css")) if !root.is_empty() => format!("{stem}.vcss_c"),
                Some((stem, "xml")) if !root.is_empty() => format!("{stem}.vxml_c"),
                Some((stem, "js")) if !root.is_empty() => format!("{stem}.vjs_c"),
                _ => format!("{path}_c"),
            };
            out.push(compiled);
        }
    }
    out.sort();
    out.dedup();
    out
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
fn deep_rules(input: &Input, referenced: &HashSet<String>, out: &mut Vec<Finding>) {
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
    let all_events = || {
        game_events.get_or_init(|| {
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
        })
    };
    // which of the game's events play each sound file, read once and only for a replaced sound encoded unlike Valve's
    let players: std::cell::OnceCell<HashMap<String, Vec<(String, Option<f64>, Option<f64>)>>> = std::cell::OnceCell::new();

    // The model rules (contract and the mesh family) are the expensive part: each model's verdict is
    // computed (or read from the findings cache) up front, the models side by side, and the file loop below
    // only reports them in file order.
    let game_id = format!("{}|{}", input.base.map_or("", |b| b.build.as_str()), input.game.map_or(String::new(), |g| g.identity()));
    // A model the game never loads (it replaces nothing and nothing in the mod names it: M031) is not on anyone's
    // screen, so nothing the model rules say of it can happen in game, fair play included (a community skin left a
    // copy of its hero under panorama/, 6871).
    let loads = |p: &str| input.base.is_none() || replaces(p) || referenced.contains(p);
    let models: Vec<&ModFile> = input.files.iter().filter(|f| ext_of(&f.path).eq_ignore_ascii_case("vmdl_c") && loads(&f.path)).collect();
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
                // M084: a replaced particle nothing in the game, and nothing in the mod, names
                if !referenced.contains(p) && input.game.and_then(|g| g.unreached_particles()).is_some_and(|u| u.contains(p)) {
                    out.push(finding("M084", Some(p), "nothing in the game names this particle (no hero, ability or item data, no model, clip, map, layout or other particle, and not the client code), and nothing in the mod does: the game never plays it, so the change does not show; find the effect you meant with `dumpparticlelist` in a match, or with `magician refs` on the effect that plays".into()));
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
                let events = |n: &str| -> Option<Value> { all_events().get(n).cloned() };
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
                // M065: another sample rate or channel count than Valve's, said with how the game's events play it
                if deep::vsnd_format(&kv).zip(deep::vsnd_format(&vk)).is_some_and(|(a, b)| a.0 != b.0 || a.1 != b.1) {
                    let index = players.get_or_init(|| {
                        let mut m: HashMap<String, Vec<(String, Option<f64>, Option<f64>)>> = HashMap::new();
                        for (name, e) in all_events() {
                            for f in deep::vsnd_refs(e) {
                                m.entry(f).or_default().push((name.clone(), e.get("pitch").and_then(Value::as_f64), e.get("volume").and_then(Value::as_f64)));
                            }
                        }
                        m.values_mut().for_each(|v| v.sort_by(|a, b| a.0.cmp(&b.0)));
                        m
                    });
                    if let Some(m) = deep::vsnd_format_parity(&kv, &vk, index.get(p).map(Vec::as_slice).unwrap_or(&[])) {
                        out.push(finding("M065", Some(p), m));
                    }
                }
            }
            "vxml_c" if replaces(p) => {
                let code = input.game.and_then(|g| g.code_names());
                let Some(valve) = game_read(p) else { continue };
                let list = |v: &[String]| {
                    let shown: Vec<&str> = v.iter().take(12).map(String::as_str).collect();
                    if v.len() > 12 { format!("{} and {} more", shown.join(", "), v.len() - 12) } else { shown.join(", ") }
                };
                if let Some(code) = code {
                    let ui = input.game.and_then(|g| g.ui_names());
                    let unknown: Vec<String> = layout::unknown_types(&f.data, &valve, code).into_iter().filter(|t| !ui.is_some_and(|u| u.contains(t))).collect();
                    if !unknown.is_empty() {
                        out.push(finding("M042", Some(p), format!("uses {} panel type(s) the game no longer registers ({}): the game cannot parse the layout ('Unrecognized panel type'), and when it is one the game requires, such as hud.xml, the game stops at launch ('FATAL ERROR: Unable to load layout file', seen on 6880) and Steam marks the install corrupt and verifies it; rebuild the layout from Valve's current file and apply the mod's change to it again", unknown.len(), list(&unknown))));
                    }
                }
                let Some((named, rest)) = layout::missing(&f.data, &valve, code) else { continue };
                if !named.is_empty() {
                    out.push(finding("M042", Some(p), format!("lacks {} element id(s) that Valve's layout on this build has and the game's client code asks for by name ({}): the game looks for them at launch and stops ('FATAL ERROR: Unable to find child', seen on 6880 for a health-bar layout without UnitHealthbar: the game never leaves its loading screen); rebuild the layout from Valve's current file and apply the mod's change to it again", named.len(), list(&named))));
                }
                if !rest.is_empty() {
                    out.push(Finding {
                        severity: Severity::Warn,
                        ..finding("M042", Some(p), format!("lacks {} element id(s) that Valve's layout on this build has ({}): {}, but Valve's scripts may look them up, and those parts of the HUD then stop updating; rebuild the layout from Valve's current file and apply the mod's change to it again", rest.len(), list(&rest), if code.is_some() { "the game's client code does not name them, so the game should still start" } else { "the game's client library could not be read, so whether its code asks for them is not known" }))
                    });
                }
            }
            "vmat_c" => {
                for m in pbrcombo::problems(&f.data) {
                    out.push(finding("M143", Some(p), m));
                }
                // M150: names no Valve material of this shader sets: the compiler kept them, the shader reads none
                if let Some(unread) = input.game.and_then(|g| g.material_params()).and_then(|mp| resource::material::from_compiled(&f.data).ok().and_then(|m| matparams::unread(&m, mp).map(|u| (m, u)))) {
                    let (m, unread) = unread;
                    if !unread.is_empty() {
                        let rim = if unread.iter().any(|n| n.to_ascii_lowercase().contains("rimlight") && !n.starts_with("g_t")) { "; a rim light's strength is the scene's, not the material's" } else { "" };
                        out.push(finding("M150", Some(p), format!("sets {} that no Valve material of {} sets on this build: the compiler keeps any name, so the shader most likely ignores {}{rim}", unread.join(", "), m.shader, if unread.len() == 1 { "it" } else { "them" })));
                    }
                }
                // M151: another shader than Valve's at this path
                if let (Ok(ours), Some(Ok(valve))) = (resource::material::from_compiled(&f.data), replaces(p).then(|| game_read(p)).flatten().map(|d| resource::material::from_compiled(&d))) {
                    if !ours.shader.eq_ignore_ascii_case(&valve.shader) {
                        let editor = if ours.shader.eq_ignore_ascii_case("environment_texture_set.vfx") { " (the CSDK's Material Editor starts new materials on it)" } else { "" };
                        out.push(finding("M151", Some(p), format!("draws with {}{editor} where Valve's material draws with {}: the features and inputs of {} are not there", ours.shader, valve.shader, valve.shader)));
                    }
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
                        let mut x = finding(code, Some(p), m.clone());
                        if *code == "M062" {
                            x.severity = deep::contract_severity(m);
                        }
                        out.push(x);
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
                // A model's morph atlas (`<model>/<set>_vmorf.vtex_c`) is the compiler's packing of that model's
                // morph targets, sized by them and read by that model alone: shipped with its own model it is no
                // picture Valve draws at Valve's size (9 on a third skin, 6871). Without its model, Valve's model
                // reads it, and it is judged.
                let own_morphs = p.strip_suffix(".vtex_c").is_some_and(|q| q.ends_with("_vmorf")) && p.rsplit_once('/').is_some_and(|(dir, _)| by_path.contains_key(format!("{dir}.vmdl_c").as_str()));
                if replaces(p) && !own_morphs {
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
        let users_of = |p: &str| {
            let mut u = g.users(p);
            u.sort();
            u.dedup();
            u
        };
        // the heroes this mod is about: those whose model it replaces, read from the users index
        let only_hero = |p: &str| Some(users_of(p)).filter(|u| u.len() == 1 && u[0].starts_with("hero_")).and_then(|mut u| u.pop());
        let heroes: std::collections::BTreeSet<String> = input.files.iter().filter(|f| replaces(&f.path) && f.path.ends_with(".vmdl_c")).filter_map(|f| only_hero(&f.path)).collect();
        for f in &input.files {
            let p = f.path.as_str();
            if !replaces(p) {
                continue;
            }
            let users = users_of(p);
            if users.len() >= 2 {
                let shown: Vec<&str> = users.iter().take(8).map(String::as_str).collect();
                let more = if users.len() > 8 { format!(" and {} more", users.len() - 8) } else { String::new() };
                out.push(finding("M090", Some(p), format!("used by {} owners: {}{more}", users.len(), shown.join(", "))));
            }
            // M092: a file only another hero uses, in a mod about this one
            if let Some(h) = only_hero(p).filter(|h| !heroes.is_empty() && !heroes.contains(h)) {
                let ours: Vec<&str> = heroes.iter().map(String::as_str).collect();
                out.push(finding("M092", Some(p), format!("used only by {h} in the game, and this mod replaces the model of {}: installing it changes {h} for everyone, which the mod is not about (a template's sample image left in the addon does this); remove it, unless the mod means to change {h} too", ours.join(", "))));
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
    // every file of the mod a judgement reads, with what the mod had there, so the cache can tell when it changes
    let deps: std::sync::Mutex<std::collections::BTreeMap<String, Fingerprint>> = Default::default();
    if let Some(valve) = &valve {
        match (deep::contract(&f.data), deep::contract(valve)) {
            (Some(a), Some(b)) => {
                // a bone is judged by what hangs on it, and reads the hero's animation from the game (whose identity
                // is in the cache key); whether the mod ships the animation skeleton is part of the judgement, so it
                // is recorded
                let ships = |q: &str| {
                    if let Ok(mut d) = deps.lock() {
                        d.insert(q.to_string(), fp(q));
                    }
                    by_path.contains_key(q)
                };
                let breaches = carry::judge(deep::contract_breaches(&a, &b), &f.data, valve, &game_read, &ships, input.game.and_then(|g| g.bone_users()));
                // a lost attachment is judged by who the game runs for this model and who names the attachment
                let owners = input.game.map(|g| g.users(p)).unwrap_or_default();
                let played = if breaches.iter().any(|m| m.starts_with("attachment '") && m.ends_with("' missing")) { carry::played(&f.data, &game_read) } else { Default::default() };
                let breaches = attach::judge(breaches, &f.data, valve, &owners, &played, input.game.and_then(|g| g.attachment_users()));
                // the camera attachments the game names, held to Valve's numbers far tighter than the printed
                // signature; one the contract already called changed or missing is not said twice
                let cams = gamedata::camera_attachments(valve, &b, input.game.and_then(|g| g.code_names()));
                let reported = |n: &str| breaches.iter().any(|m| m.starts_with(&format!("attachment '{n}' ")));
                let cam = gamedata::camera_breaches(&f.data, valve, &cams, &reported);
                for m in deep::judge_breaches(breaches, &f.data, valve) {
                    out.push(("M062", m));
                }
                out.extend(cam.into_iter().map(|m| ("M062", m)));
                // the animation references and the game data blocks Deadlock reads from the model
                let has = |n: &str| a.attachments.iter().any(|(k, _)| k == n);
                out.extend(gamedata::judge(&f.data, valve, &has).into_iter().map(|m| ("M062", m)));
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
        // M160: Valve's cloth or ragdoll parts that ours dropped
        out.extend(physdrop::judge(&f.data, valve).into_iter().map(|m| ("M160", m)));
    }
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
