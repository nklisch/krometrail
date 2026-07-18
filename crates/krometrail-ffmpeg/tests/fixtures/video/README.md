# MP4/H.264 validator fixture

`valid-h264.mp4` is a two-pixel-square, silent, 350 ms H.264/MP4 qualification fixture. It was
generated locally with the user-installed Homebrew FFmpeg 8.0.1 build (`libx264`) on 2026-07-18;
no FFmpeg executable is stored or redistributed by this repository.

The generation policy fixed one video stream, no audio/subtitle/data streams, `yuv420p`, all-intra
`libx264`, one encoder thread, a 1 MHz track timebase, stripped metadata, a 350 ms output duration,
and fast-start MP4 layout. `ffprobe` reported one H.264 video stream, 2x2 pixels, time base
`1/1000000`, and duration `0.350000`. The file's SHA-256 is
`beb459d999100e32a752cb62519de47fc2e053513916a69c43bfd2698ceb0188`.

Regenerate only through the ignored opt-in real-FFmpeg qualification workflow, then update this
provenance and the hash together. Default tests only read the retained bytes and never invoke
FFmpeg or contact a network.
