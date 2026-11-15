# Test recordings

Recordings for the audio regression tests in [`core/src/audio_tests.rs`](../../src/audio_tests.rs) and for
the packaged-app smoke test. Each `.wav` (16 kHz, mono, 16-bit) has a `.txt` beside it with the words spoken.

- `clips/` — twelve recordings of four to eight seconds, the length of an ordinary dictation
- `short/` — eight recordings of two to three seconds

## Source and licence

The recordings come from the `test-other` part of [LibriSpeech](https://www.openslr.org/12), a corpus of read
English speech from LibriVox audiobooks, by Vassil Panayotov, Guoguo Chen, Daniel Povey and Sanjeev Khudanpur.
LibriSpeech is licensed under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). The audio was
converted from FLAC to WAV; nothing else was changed. `test-other` is the harder half of the corpus: more
accents, more background noise, less careful diction.

| File | LibriSpeech utterance | Length | Words |
| --- | --- | --- | --- |
| `clips/clip-01.wav` | 7902-96591-0002 | 5.0 s | 13 |
| `clips/clip-02.wav` | 7902-96591-0007 | 7.7 s | 23 |
| `clips/clip-03.wav` | 7018-75788-0001 | 5.9 s | 15 |
| `clips/clip-04.wav` | 7018-75788-0002 | 6.1 s | 16 |
| `clips/clip-05.wav` | 4198-12281-0004 | 5.3 s | 14 |
| `clips/clip-06.wav` | 4198-12281-0006 | 4.5 s | 11 |
| `clips/clip-07.wav` | 3080-5040-0000 | 6.8 s | 18 |
| `clips/clip-08.wav` | 3080-5040-0012 | 7.0 s | 18 |
| `clips/clip-09.wav` | 8280-266249-0004 | 5.9 s | 17 |
| `clips/clip-10.wav` | 8280-266249-0006 | 6.2 s | 17 |
| `clips/clip-11.wav` | 5764-299665-0002 | 4.0 s | 6 |
| `clips/clip-12.wav` | 5764-299665-0005 | 4.7 s | 8 |
| `short/short-01.wav` | 7902-96591-0000 | 2.1 s | 9 |
| `short/short-02.wav` | 7902-96591-0001 | 2.5 s | 9 |
| `short/short-03.wav` | 4198-12281-0001 | 2.6 s | 6 |
| `short/short-04.wav` | 4198-12259-0014 | 2.5 s | 4 |
| `short/short-05.wav` | 3080-5040-0009 | 2.6 s | 7 |
| `short/short-06.wav` | 3080-5040-0017 | 1.9 s | 5 |
| `short/short-07.wav` | 8280-266249-0009 | 2.8 s | 9 |
| `short/short-08.wav` | 8280-266249-0010 | 2.7 s | 7 |

## Changing them

The tests compare each model's error rate on these exact files with limits in `audio_tests.rs`. If you add,
remove or replace a recording, run `npm run core:test:audio` (see the top of that file) and adjust the limits
to what you measure, with some headroom.
