# MP4/H.264 validator fixture

`valid-h264.mp4` is a two-pixel-square, silent, 350 ms H.264/MP4 qualification fixture. It was
generated locally with the user-installed Homebrew FFmpeg 8.0.1 build (`libx264`) on 2026-07-18;
no FFmpeg executable is stored or redistributed by this repository.

The generation policy fixed one video stream, no audio/subtitle/data streams, `yuv420p`, all-intra
`libx264`, one encoder thread, a 1 MHz track timebase, stripped metadata, a 350 ms output duration,
and fast-start MP4 layout. Its three samples carry the two source frames plus the terminal
sentinel at microsecond PTS `[0, 100000, 349999]`. `ffprobe` reported one H.264 video stream,
2x2 pixels, time base `1/1000000`, duration `0.350000`, and three frames. The file's SHA-256 is
`8b3905f2acd80fc1f4c2a476e8339ca0c17d79c0efa668ba0f6894f6fad2c762`.

Regenerate only through the ignored opt-in real-FFmpeg qualification workflow with an explicit
`KROMETRAIL_FFMPEG_FIXTURE_OUTPUT` path, then update this provenance and the hash together.
Default tests only read the retained bytes and never invoke FFmpeg or contact a network.

`terminal-hold-zero-h264.mp4` is the corresponding Fedora FFmpeg 7.1.2 exemplar. It was produced
by the exact qualification shape: two staged 2x2 RGBA PNGs and an `ffconcat` timeline, encoded with
the policy's `libx264`, `yuv420p`, 1 MHz track timebase, all-intra, fast-start MP4, and 350 ms
duration arguments. Its `stts` entries are `[(1,100000),(1,249999),(1,0)]`; the muxer-defined
terminal hold is stored as zero while the leading deltas remain exact. The file is 1508 bytes and
its SHA-256 is `b3ac999ae30d653ea4ca1e19a5102154db9e0546d8f620794d84f5a4a4d32b50`.
