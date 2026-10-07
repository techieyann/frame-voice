#!/usr/bin/env python3
"""Developer helper: render existing tray vectors as embedded 64px ARGB pixmaps."""
from pathlib import Path
import gi

gi.require_version('GdkPixbuf', '2.0')
from gi.repository import GdkPixbuf

root = Path(__file__).resolve().parent.parent/'assets/icons'
for source, target in (('tray-microphone.svg', 'frame-voice-active.argb'),
                       ('tray-muted-microphone.svg', 'frame-voice-stopped.argb')):
    pixbuf = GdkPixbuf.Pixbuf.new_from_file_at_size(str(root/source), 64, 64)
    assert pixbuf.get_width() == pixbuf.get_height() == 64
    rgba = pixbuf.get_pixels()
    stride, channels = pixbuf.get_rowstride(), pixbuf.get_n_channels()
    pixels = bytearray()
    for y in range(64):
        for x in range(64):
            i = y*stride + x*channels
            r, g, b = rgba[i:i+3]
            a = rgba[i+3] if channels == 4 else 255
            pixels.extend((a, r, g, b))
    (root/target).write_bytes(pixels)
