# Open in slicer

Model files (STL, 3MF, OBJ, STEP) have an **Open in slicer** button, in the file list and at
the top of the project page. It hands the file's direct URL
(`/media/library/<folder>/<file>`) to the slicer through the slicer's URL scheme:

| Slicer | Link |
|--------|------|
| Bambu Studio | `bambustudio://open?file=<url>` |
| OrcaSlicer | `orcaslicer://open?file=<url>` |
| PrusaSlicer | `prusaslicer://open?file=<url>` |

The last slicer used becomes the default (stored per browser). Every attempt shows a toast with
a **Download** button, because the browser can't tell whether the slicer actually opened.

## What can go wrong

- **The slicer isn't installed, or the scheme isn't registered.** Nothing happens. On Linux
  desktops the scheme often isn't registered even when the slicer is installed (KDE reports
  "unknown protocol"). Use Download.
- **The slicer refuses LiMa's address.** Slicers only download from sites they trust:
  - PrusaSlicer only opens links from domains Prusa has reviewed and allowlisted
    (Printables and a few others). There is no setting to add your own, so a self-hosted
    LiMa is refused. You also need *Preferences → Allow downloads from supported websites* on.
  - Bambu Studio's handling isn't publicly documented. Expect it to accept MakerWorld links only.
  - OrcaSlicer has no documented domain restriction, but users report downloads failing on
    some versions.
- **The slicer can't reach the URL.** The slicer downloads the file itself, so LiMa's address
  must be reachable from the computer running the slicer (not `localhost` on another machine).

## Workaround for allowlists

A small local helper can receive the link, download the file and open it in the slicer.
[slicer-uri-bridge](https://github.com/mbv06/slicer-uri-bridge) does this for Bambu Studio and
registers itself for the other slicers' schemes. LiMa doesn't ship or require it.

This was written without testing against each slicer release. If you find a slicer and
version that works (or doesn't) with a self-hosted LiMa, add it here.
