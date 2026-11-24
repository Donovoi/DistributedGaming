"""Run in the capture image on a remote PC or CI VM."""

import unittest

from smoke import require_varied_pixels


class ColorDataTests(unittest.TestCase):
    def test_uniform_pixel_with_four_distinct_channels_is_rejected(self):
        with self.assertRaisesRegex(RuntimeError, "flat"):
            require_varied_pixels(bytes([10, 20, 30, 255]) * 4, 2, 2)

    def test_incomplete_pixel_data_is_rejected(self):
        with self.assertRaisesRegex(RuntimeError, "length"):
            require_varied_pixels(bytes(range(15)), 2, 2)

    def test_empty_image_is_rejected(self):
        with self.assertRaisesRegex(RuntimeError, "length"):
            require_varied_pixels(b"", 0, 0)

    def test_distinct_pixels_are_accepted(self):
        require_varied_pixels(bytes([10, 20, 30, 255, 11, 20, 30, 255]), 2, 1)


if __name__ == "__main__":
    unittest.main()
