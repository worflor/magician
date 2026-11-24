"""Write the cinema kit's studio lights: particle systems that are nothing but dynamic lights.

    python -I studio_lights.py            # writes addon/particles/cinema/*.vpcf, then magician build cinema_dev

Retail has no way to place a light: `ent_create light_omni2|light_rect|light_spot|light_barn` spawns the entity
but takes no keyvalues (ent_create only takes a class name), and SetColor / SetBrightness / Enable / TurnOn on it
light nothing (build 6864). Particles can carry lights (C_OP_RenderOmni2Light, the renderer behind Valve's hideout
fireplace and our own ability lights), and `particle_test_file` + `particle_test_start` play a particle on the
entity under the crosshair. So a light rig is a particle whose children each hold one light at an offset in the
entity's own frame (x forward, y left, z up from its feet): aim at the hero, start the rig, and the key, fill and
rim are placed round the hero where it stands (they don't follow it if it moves). Each light lives until
`particle_test_stop`.

Rigs (all positions in units from the hero's origin, its facing as +x):
    studio_rig     key warm front-left high, fill cool front-right low, rim white behind and above
    studio_key     the key alone
    studio_rim     two rims (behind left and right), for a silhouette on the black void
    studio_top     one hard light straight above (the "interrogation" or stage-spot look)
"""
import os

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "addon", "particles", "cinema")

HEADER = ("<!-- kv3 encoding:text:version{e21c7f3c-8a33-41c5-9977-a76d3a32aa0d} "
          "format:vpcf61:version{39d3ce1d-9442-4407-9b78-8317145b1732} -->\n")

# name: (offset xyz, colour rgb, lumens, range, shadows)
LIGHTS = {
    "key":       ((90, 70, 120), (255, 226, 190), 30000, 520, True),
    "fill":      ((80, -90, 60), (170, 190, 255), 7000, 480, False),
    "rim":       ((-90, -20, 150), (255, 255, 255), 26000, 480, False),
    "rim_left":  ((-80, 70, 130), (255, 240, 225), 22000, 440, False),
    "rim_right": ((-80, -70, 130), (225, 235, 255), 22000, 440, False),
    "top":       ((0, 0, 230), (255, 245, 230), 40000, 420, True),
}
RIGS = {
    "studio_rig": ["key", "fill", "rim"],
    "studio_key": ["key"],
    "studio_rim": ["rim_left", "rim_right"],
    "studio_top": ["top"],
}


def light(name):
    (x, y, z), (r, g, b), lumens, rng, shadows = LIGHTS[name]
    return HEADER + f"""{{
	_class = "CParticleSystemDefinition"
	m_nMaxParticles = 1
	m_nBehaviorVersion = 12
	m_Emitters =
	[
		{{
			_class = "C_OP_InstantaneousEmitter"
			m_nParticlesToEmit = {{ m_nType = "PF_TYPE_LITERAL" m_flLiteralValue = 1.0 }}
		}},
	]
	m_Initializers =
	[
		{{
			_class = "C_INIT_CreateWithinSphereTransform"
		}},
		{{
			_class = "C_INIT_PositionOffset"
			m_bLocalCoords = true
			m_OffsetMin = {{ m_nType = "PVEC_TYPE_LITERAL" m_vLiteralValue = [ {x:.1f}, {y:.1f}, {z:.1f} ] }}
			m_OffsetMax = {{ m_nType = "PVEC_TYPE_LITERAL" m_vLiteralValue = [ {x:.1f}, {y:.1f}, {z:.1f} ] }}
		}},
	]
	m_Renderers =
	[
		{{
			_class = "C_OP_RenderOmni2Light"
			m_flRange = {{ m_nType = "PF_TYPE_LITERAL" m_flLiteralValue = {rng:.1f} }}
			m_bCastShadows = {"true" if shadows else "false"}
			m_vColorBlend = {{ m_nType = "PVEC_TYPE_LITERAL_COLOR" m_LiteralColor = [ {r}, {g}, {b} ] }}
			m_flBrightnessLumens = {{ m_nType = "PF_TYPE_LITERAL" m_flLiteralValue = {lumens:.1f} }}
		}},
	]
}}
"""


def rig(children):
    kids = "".join(f'\t\t{{\n\t\t\tm_ChildRef = resource:"particles/cinema/light_{c}.vpcf"\n\t\t}},\n' for c in children)
    return HEADER + f"""{{
	_class = "CParticleSystemDefinition"
	m_nMaxParticles = 1
	m_nBehaviorVersion = 12
	m_Children =
	[
{kids}	]
}}
"""


def main():
    os.makedirs(OUT, exist_ok=True)
    written = []
    for name in LIGHTS:
        written.append((f"light_{name}.vpcf", light(name)))
    for name, kids in RIGS.items():
        written.append((f"{name}.vpcf", rig(kids)))
    for fn, text in written:
        with open(os.path.join(OUT, fn), "w", encoding="utf-8", newline="\n") as f:   # no BOM: resourcecompiler rejects one
            f.write(text)
    print(f"wrote {len(written)} particle files to {OUT}")


if __name__ == "__main__":
    main()
