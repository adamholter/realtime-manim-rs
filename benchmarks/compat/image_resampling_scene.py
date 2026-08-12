import numpy as np
from PIL import Image
from manim import *


class ImageResamplingCompatibility(Scene):
    def construct(self):
        pixels = np.zeros((10, 10, 4), dtype=np.uint8)
        pixels[:, :, 3] = 255
        for row in range(10):
            for column in range(10):
                pixels[row, column, :3] = (
                    [245, 65, 145]
                    if (row + column) % 2 == 0
                    else [35, 205, 245]
                )
        filters = [
            Image.Resampling.NEAREST,
            Image.Resampling.BOX,
            Image.Resampling.BILINEAR,
            Image.Resampling.HAMMING,
            Image.Resampling.BICUBIC,
            Image.Resampling.LANCZOS,
        ]
        positions = [
            [-4.5, 1.6, 0.0],
            [0.0, 1.6, 0.0],
            [4.5, 1.6, 0.0],
            [-4.5, -1.6, 0.0],
            [0.0, -1.6, 0.0],
            [4.5, -1.6, 0.0],
        ]
        images = Group()
        for algorithm, position in zip(filters, positions, strict=True):
            image = ImageMobject(pixels)
            image.resampling_algorithm = algorithm
            image.set(height=2.4).move_to(position)
            images.add(image)
        self.add(images)
        self.wait(0.25)
