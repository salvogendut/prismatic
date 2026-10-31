# UI sound assets

`shutter.wav` is the shared Happy Years screenshot sample, extracted byte for
byte from `../1984/src/shutter_wav.h`. The copies in 1985 and 1986 are identical.
It is embedded in the executable and played through a dedicated SDL3 stream
after a successful F4 screenshot save.

Format: mono, 44.1 kHz, signed 16-bit PCM (14,976 samples).
