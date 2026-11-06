# Media

The board film: a 20 to 30 second recording of the Pantry board, made by
[`scripts/board-video.mjs`](../../scripts/board-video.mjs) from a running
Prepcook in the light theme at 1440×900. It shows the board, the Plan bug's
reproduction and RCA, the plan approved into Execute, the Review card, and the
merged card cleaned up out of Shipped.

| File | What it is | Used by |
| --- | --- | --- |
| `board.webm` | The recording, cut to the storyboard (VP9). | The website's home page, which plays it beside "Nothing merges without a person" when it exists at build time. |
| `board.mp4` | H.264, yuv420p, 1440 wide, with the index at the front. | The home page, as the second source (Safari). |
| `board.gif` | 960 wide at 12 fps, under 6 MB (10 fps, then 800 wide, when it would not be). | The README, under the board screenshot. |

Without a system `ffmpeg` on the `PATH` the script writes `board.webm` alone,
uncut, so install it first (`brew install ffmpeg`).
[docs/images/README.md](../images/README.md#video) says how to run it.
