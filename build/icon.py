# Generates build/icon.png: side view of a Gaussian beam, |E(r, z)| in the magma colormap on a transparent background.
# Run with: python build/icon.py   (needs numpy, matplotlib, pillow)
import numpy as np
import matplotlib
matplotlib.use('Agg')
from matplotlib import cm
from PIL import Image

N = 1024
w0, zR = 0.12, 0.36  # waist radius and Rayleigh range, in units of half the canvas
z = np.linspace(-1, 1, N)
Z, R = np.meshgrid(z, z)
W = w0 * np.sqrt(1 + (Z / zR) ** 2)
I = (w0 / W) * np.exp(-(R / W) ** 2)
I /= I.max()
# Below c0 the colour stays at magma's purple and only the opacity fades, so the edge reads well on light and dark.
c0 = 0.3
rgba = cm.magma(np.maximum(I, c0))
rgba[..., 3] = np.clip(I / c0, 0, 1) ** 1.2 * np.clip((1 - np.abs(Z)) / 0.12, 0, 1)
Image.fromarray((rgba * 255).astype(np.uint8), 'RGBA').save('build/icon.png')
