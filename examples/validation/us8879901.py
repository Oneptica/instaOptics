"""Reference data for the US 8,879,901 objective (11 elements, f/2, 21.6° full field) from optiland, an independent
open-source ray tracer (MIT, https://github.com/HarrisonKramer/optiland).

Writes into crates/engine/tests/fixtures/us8879901/:
  us8879901.zmx      the lens as OpticStudio writes it (optiland's Zemax exporter)
  US8879901.agf      its nine glasses: optiland's dispersion data refitted to the AGF Sellmeier formula
  reference.json     paraxial data and real rays at the image traced by optiland

    pip install optiland
    python examples/validation/us8879901.py
"""
import json
import os

import numpy as np
from scipy.optimize import least_squares

from optiland.fileio.zemax.writer.exporter import save_zemax_file
from optiland.samples.objectives import ObjectiveUS008879901

OUT = os.path.join(os.path.dirname(__file__), "..", "..", "crates", "engine", "tests", "fixtures", "us8879901")
CATALOG = "US8879901"
LAMBDA = np.linspace(0.40, 0.80, 81)
D, F, C = 0.5875618, 0.4861327, 0.6562725


def sellmeier(p, l):
    l2 = l * l
    return np.sqrt(1 + sum(p[2 * k] * l2 / (l2 - p[2 * k + 1]) for k in range(3)))


def fit(material):
    n = np.asarray(material.n(LAMBDA), dtype=float).ravel()
    start = [1.0, 0.006, 0.3, 0.02, 1.0, 100.0]
    r = least_squares(lambda p: sellmeier(p, LAMBDA) - n, start, x_scale="jac", xtol=1e-15, ftol=1e-15, gtol=1e-15, max_nfev=20000)
    return r.x, float(np.max(np.abs(r.fun)))


def main():
    os.makedirs(OUT, exist_ok=True)
    lens = ObjectiveUS008879901()

    glasses, lines = {}, ["CC Glasses of the US 8,879,901 objective, refitted from optiland's data (Sellmeier, 0.40-0.80 um)"]
    for s in lens.surfaces:
        m = s.material_post
        name = getattr(m, "name", None)
        if not name or name in glasses or not hasattr(m, "n"):
            continue
        p, err = fit(m)
        nd, nf, nc = (float(np.asarray(m.n(w)).ravel()[0]) for w in (D, F, C))
        glasses[name] = err
        lines.append(f"NM {name} 2 0 {nd:.6f} {(nd - 1) / (nf - nc):.4f} 0 0")
        lines.append("CD " + " ".join(f"{v:.12e}" for v in p) + " 0 0 0 0")
        lines.append(f"LD {LAMBDA[0]:.2f} {LAMBDA[-1]:.2f}")
    with open(os.path.join(OUT, f"{CATALOG}.agf"), "w") as f:
        f.write("\n".join(lines) + "\n")
    print("glass fit, max |Δn|:", {k: f"{v:.1e}" for k, v in glasses.items()})

    save_zemax_file(lens, os.path.join(OUT, "us8879901.zmx"))

    p = lens.paraxial
    last_thickness = float(lens.surfaces[len(lens.surfaces) - 2].thickness)
    paraxial = {
        "efl": float(p.f2()),
        "bfl": float(p.F2()) + last_thickness,
        "fNumber": float(p.FNO()),
        "entrancePupilDiameter": float(p.EPD()),
        "entrancePupilZ": float(p.EPL()),
        "exitPupilFromImage": float(p.XPL()),
    }
    fields = [0.0, 7.574, 10.82]
    wavelengths = [0.4861327, 0.5875618, 0.6562725]
    pupil = [(0.0, 0.0), (0.0, 1.0), (0.0, -1.0), (1.0, 0.0), (0.0, 0.5), (0.7, 0.7)]
    rays = []
    for fi, angle in enumerate(fields):
        for wi, w in enumerate(wavelengths):
            for px, py in pupil:
                r = lens.trace_generic(0.0, angle / fields[-1], px, py, w)
                rays.append({"field": fi, "wavelength": wi, "px": px, "py": py, "x": float(np.asarray(r.x).ravel()[0]), "y": float(np.asarray(r.y).ravel()[0])})
    with open(os.path.join(OUT, "reference.json"), "w") as f:
        json.dump({"source": "optiland " + __import__("importlib.metadata").metadata.version("optiland"), "rayAiming": "paraxial", "paraxial": paraxial, "rays": rays}, f, indent=1)
    print("paraxial:", paraxial)


if __name__ == "__main__":
    main()
