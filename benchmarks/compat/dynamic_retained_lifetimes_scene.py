import numpy as np
from PIL import Image
from manim import *


class DynamicRetainedLifetimesCompatibility(Scene):
    def construct(self):
        pixels = np.zeros((12, 12, 4), dtype=np.uint8)
        pixels[:, :, 0] = 30
        pixels[:, :, 1] = 160
        pixels[:, :, 2] = 240
        pixels[:, :, 3] = 255
        image = ImageMobject(pixels).scale(1.7).shift(LEFT * 2.8)

        cloud = PMobject(stroke_width=18)
        cloud.points = np.array(
            [[-0.5, -0.4, 0.0], [0.5, 0.4, 0.0]], dtype=float
        )
        cloud.rgbas = np.array(
            [RED.to_rgba(), GREEN.to_rgba()], dtype=float
        )

        square = Square(side_length=1.4, color=YELLOW).shift(RIGHT * 2.8)
        self.add(image, cloud, square)
        filters = [
            Image.Resampling.NEAREST,
            Image.Resampling.BOX,
            Image.Resampling.BILINEAR,
            Image.Resampling.HAMMING,
            Image.Resampling.BICUBIC,
            Image.Resampling.LANCZOS,
        ]

        def change_static_layouts(_group, alpha):
            channel = int(round(30 + 190 * alpha))
            image.pixel_array[:, :, 0] = channel
            image.pixel_array[:, :, 3] = np.linspace(
                40 + int(80 * alpha), 255, 12, dtype=np.uint8
            )[:, None]
            image.resampling_algorithm = filters[
                min(int(alpha * len(filters)), len(filters) - 1)
            ]
            if alpha < 0.5:
                cloud.points = np.array(
                    [[-0.5, -0.4, 0.0], [0.5, 0.4, 0.0]], dtype=float
                )
                cloud.rgbas = np.array(
                    [RED.to_rgba(), GREEN.to_rgba()], dtype=float
                )
            else:
                cloud.points = np.array(
                    [
                        [-0.7, -0.5, 0.0],
                        [-0.2, 0.55, 0.0],
                        [0.3, -0.25, 0.0],
                        [0.75, 0.45, 0.0],
                    ],
                    dtype=float,
                )
                cloud.rgbas = np.array(
                    [
                        RED.to_rgba(),
                        BLUE.to_rgba(),
                        GREEN.to_rgba(),
                        ORANGE.to_rgba(),
                    ],
                    dtype=float,
                )

        group = Group(image, cloud)
        self.play(
            UpdateFromAlphaFunc(group, change_static_layouts), run_time=0.8
        )
        self.remove(square)
        self.wait(0.2)
        self.add(square)
        self.play(square.animate.shift(UP * 0.8), run_time=0.4)
