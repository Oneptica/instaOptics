"""Records the README animation: starts instaOptics with its control channel, drives it through a short design story
and turns the window screenshots into docs/screenshots/demo.gif with a caption under each step.

    python examples/demo/record.py [--app target/release/instaoptics] [--out docs/screenshots/demo.gif]

Needs Pillow and a display. Every step is an ordinary command, the same ones the menus and MCP run.
"""
import argparse
import json
import os
import socket
import subprocess
import tempfile
import time

from PIL import Image, ImageDraw, ImageFont

PORT = 7393


class App:
    def __init__(self, exe):
        self.proc = subprocess.Popen([exe, "--control", str(PORT)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        runtime = os.environ.get("XDG_RUNTIME_DIR") or os.path.expanduser("~/.cache")
        token_file = os.path.join(runtime, f"instaoptics-control-{PORT}.token")
        for _ in range(100):
            time.sleep(0.2)
            try:
                self.token = open(token_file).read().strip()
                self.io = socket.create_connection(("127.0.0.1", PORT)).makefile("rw")
                return
            except OSError:
                pass
        raise RuntimeError("instaOptics did not start its control channel")

    def call(self, method, params=None):
        self.io.write(json.dumps({"id": 1, "method": method, "params": params or {}, "token": self.token}) + "\n")
        self.io.flush()
        reply = json.loads(self.io.readline())
        if not reply.get("ok"):
            raise RuntimeError(f"{method}: {reply.get('error')}")
        return reply.get("result")

    def run(self, command, **params):
        return self.call("engine.execute", {"command": command, "params": params})

    def close(self):
        self.proc.terminate()


class Recorder:
    def __init__(self, app, folder):
        self.app, self.folder, self.frames = app, folder, []

    def shot(self, caption, hold=1, settle=0.35):
        """One frame shown for `hold` GIF ticks, after the analyses have had time to update."""
        time.sleep(settle)
        path = os.path.join(self.folder, f"{len(self.frames):04d}.png")
        self.app.call("ui.screenshot", {"path": path})
        self.frames.append((path, caption, hold))


def caption_bar(img, text, font):
    w, h = img.size
    bar = 44
    out = Image.new("RGB", (w, h + bar), (24, 24, 24))
    out.paste(img, (0, 0))
    d = ImageDraw.Draw(out)
    d.text((16, h + bar / 2), text, font=font, fill=(230, 230, 230), anchor="lm")
    return out


def font(size):
    for path in ["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "/Library/Fonts/Arial.ttf", "C:/Windows/Fonts/segoeui.ttf"]:
        if os.path.exists(path):
            return ImageFont.truetype(path, size)
    return ImageFont.load_default()


def story(app, rec):
    app.call("ui.resize", {"width": 1280, "height": 760})
    app.run("sample", id="cooke")
    for w in ["layout", "spot", "lensData"]:
        app.call("ui.open", {"window": w})
    rec.shot("A Cooke triplet: Lens Data, 2D layout and spot diagram", hold=18, settle=1.5)

    caption = "Bend the first lens: radius of surface 2 from −436 to −120 mm, every analysis follows"
    for r in [-435.76, -330, -250, -190, -150, -120]:
        app.run("surface.set", surface=2, radius=r)
        rec.shot(caption, hold=3)
    rec.shot(caption, hold=8)

    caption = "Optimize: R2, R4 and back focus variable, EFL held at 50 mm, damped least squares on the RMS spot"
    app.run("system.set", targetEfl=50)
    for s, p in [(2, "radius"), (4, "radius"), (6, "thickness")]:
        app.run("variable", surface=s, parameter=p)
    app.call("ui.open", {"window": "optimize"})
    rec.shot(caption, hold=10, settle=0.8)
    for _ in range(5):
        app.run("optimize", cycles=1)
        rec.shot(caption, hold=4)
    rec.shot(caption, hold=8)

    app.call("ui.open", {"window": "spot"})
    rec.shot("Spot diagram after optimization", hold=16, settle=0.8)
    app.call("ui.open", {"window": "mtf"})
    rec.shot("FFT MTF of the optimized lens", hold=14, settle=1.0)
    app.call("ui.open", {"window": "layout3d"})
    rec.shot("3D layout", hold=14, settle=1.0)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--app", default="target/release/instaoptics")
    parser.add_argument("--out", default="docs/screenshots/demo.gif")
    parser.add_argument("--width", type=int, default=960)
    parser.add_argument("--tick", type=int, default=100, help="ms per GIF tick")
    args = parser.parse_args()
    app = App(args.app)
    with tempfile.TemporaryDirectory() as folder:
        rec = Recorder(app, folder)
        try:
            story(app, rec)
        finally:
            app.close()
        f = font(15)
        frames, durations = [], []
        for path, caption, hold in rec.frames:
            img = Image.open(path).convert("RGB")
            img = img.resize((args.width, round(img.height * args.width / img.width)), Image.LANCZOS)
            frames.append(caption_bar(img, caption, f))
            durations.append(hold * args.tick)
    # One shared palette keeps the GIF small and stops the colours flickering between frames.
    palette = frames[0].quantize(colors=255, method=Image.Quantize.MEDIANCUT)
    gif = [fr.quantize(palette=palette, dither=Image.Dither.NONE) for fr in frames]
    gif[0].save(args.out, save_all=True, append_images=gif[1:], duration=durations, loop=0, optimize=True)
    print(f"wrote {args.out} ({len(gif)} frames, {sum(durations) / 1000:.1f} s, {os.path.getsize(args.out) / 1e6:.1f} MB)")


if __name__ == "__main__":
    main()
