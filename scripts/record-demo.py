#!/usr/bin/env python3
"""Record a diple session into the animated GIF shown in the README.

Drives the real binary inside a pty, emulates the terminal with pyte and
renders the resulting screens with Pillow. Only frames of a quiescent screen
are sampled, so no half-painted redraw ends up in the GIF.

    python3 -m venv /tmp/demo-venv
    /tmp/demo-venv/bin/pip install pyte pillow
    cargo build --release
    /tmp/demo-venv/bin/python scripts/record-demo.py \
        target/release/diple tests/fixtures/readme.md docs/demo.gif

Set DUMP=<dir> to also write every frame as a PNG for inspection.
"""
import fcntl, os, pty, struct, subprocess, sys, termios, time
import pyte
from PIL import Image, ImageDraw, ImageFont

COLS, ROWS = 100, 30
FONT_PATH = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"
FONT_SIZE = 17
SAMPLE = 0.06          # seconds between screen samples

BINARY = os.path.abspath(sys.argv[1])
DOC = os.path.abspath(sys.argv[2])
OUT = os.path.abspath(sys.argv[3])

# (delay before sending, bytes to send, comment)
SCRIPT = [
    (1.8, b"",   "opening view"),
    (0.6, b"j",  "scroll"),
    (0.25, b"j", ""),
    (0.25, b"j", ""),
    (1.0, b"]",  "next heading"),
    (0.9, b"]",  ""),
    (0.9, b"]",  ""),
    (1.2, b"K",  "key hints sidebar"),
    (3.0, b"K",  "close hints"),
    (1.0, b"t",  "table of contents"),
    (1.1, b"j",  ""),
    (0.5, b"j",  ""),
    (0.5, b"j",  ""),
    (0.9, b"\r", "jump to heading"),
    (1.4, b"j",  "the focus stays in the sidebar"),
    (0.5, b"j",  ""),
    (0.7, b"\r", "so the next jump needs no reopening"),
    (1.6, b"\x1b", "close toc"),
    (0.9, b"/",  "search"),
    (0.7, b"c", ""), (0.14, b"o", ""), (0.14, b"n", ""), (0.14, b"f", ""),
    (0.14, b"i", ""), (0.14, b"g", ""),
    (0.9, b"\r", "run search"),
    (1.5, b"n",  "next match"),
    (1.5, b"n",  ""),
    (1.6, b"zM", "collapse all"),
    (1.6, b"\r", "expand the top level"),
    (2.2, b"zR", "expand all"),
    (1.4, b"g",  "back to top"),
    (2.0, b"q",  "quit"),
    (0.6, b"",   "tail"),
]


def run():
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
    env = dict(os.environ,
               TERM="xterm-256color", COLORTERM="truecolor",
               LINES=str(ROWS), COLUMNS=str(COLS), NO_COLOR="")
    env.pop("NO_COLOR")
    # Run from the document's directory and pass the bare file name: the
    # status bar shows the path it was given, and a long absolute one would
    # crowd out everything else on that row.
    proc = subprocess.Popen([BINARY, os.path.basename(DOC)],
                            cwd=os.path.dirname(DOC), stdin=slave,
                            stdout=slave, stderr=slave, env=env,
                            close_fds=True)
    os.close(slave)
    os.set_blocking(master, False)

    screen = pyte.Screen(COLS, ROWS)
    stream = pyte.ByteStream(screen)
    frames = []          # (snapshot, duration)

    def snapshot():
        rows = []
        for y in range(ROWS):
            line = screen.buffer[y]
            rows.append(tuple((line[x].data or " ", line[x].fg, line[x].bg,
                               line[x].bold, line[x].reverse) for x in range(COLS)))
        return tuple(rows)

    import select

    def pump(seconds):
        end = time.monotonic() + seconds
        last = time.monotonic()
        while True:
            now = time.monotonic()
            if now >= end:
                break
            r, _, _ = select.select([master], [], [], min(SAMPLE, end - now))
            busy = False
            if r:
                try:
                    data = os.read(master, 65536)
                except OSError:
                    data = b""
                if data:
                    stream.feed(data)
                    busy = True
            now = time.monotonic()
            elapsed, last = now - last, now
            if frames:
                frames[-1][1] += elapsed
            if busy:
                # mid-redraw: the screen may be half-painted, do not sample it
                continue
            snap = snapshot()
            if not frames or frames[-1][0] != snap:
                frames.append([snap, 0.0])

    for delay, keys, _c in SCRIPT:
        pump(delay)
        if keys:
            os.write(master, keys)
    pump(0.4)
    try:
        proc.wait(timeout=2)
    except subprocess.TimeoutExpired:
        proc.kill()
    os.close(master)
    return frames


PALETTE = {
    "black": (0x1c, 0x1f, 0x26), "red": (0xe0, 0x60, 0x6a),
    "green": (0x69, 0xc2, 0x7a), "brown": (0xd6, 0xa2, 0x4a),
    "yellow": (0xd6, 0xa2, 0x4a), "blue": (0x5d, 0x9c, 0xec),
    "magenta": (0xc0, 0x83, 0xd6), "cyan": (0x4f, 0xb5, 0xba),
    "white": (0xd8, 0xdc, 0xe3),
    "brightblack": (0x6b, 0x72, 0x80), "brightred": (0xf2, 0x7d, 0x86),
    "brightgreen": (0x8a, 0xd6, 0x97), "brightbrown": (0xe8, 0xc0, 0x6e),
    "brightyellow": (0xe8, 0xc0, 0x6e), "brightblue": (0x7d, 0xb4, 0xf5),
    "brightmagenta": (0xd4, 0xa2, 0xe8), "brightcyan": (0x74, 0xcf, 0xd4),
    "brightwhite": (0xff, 0xff, 0xff),
}
BG = (0x14, 0x17, 0x1c)
FG = (0xd8, 0xdc, 0xe3)


def color(name, default):
    if name == "default":
        return default
    if name in PALETTE:
        return PALETTE[name]
    try:
        return tuple(int(name[i:i + 2], 16) for i in (0, 2, 4))
    except Exception:
        return default


def render(frames, out):
    font = ImageFont.truetype(FONT_PATH, FONT_SIZE)
    bold = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf", FONT_SIZE)
    cw = round(font.getlength("M"))
    ascent, descent = font.getmetrics()
    ch = ascent + descent
    pad = 14
    W, H = cw * COLS + 2 * pad, ch * ROWS + 2 * pad
    images, durations = [], []
    dump = os.environ.get("DUMP")
    if dump:
        os.makedirs(dump, exist_ok=True)
    for i, (snap, dur) in enumerate(frames):
        img = Image.new("RGB", (W, H), BG)
        d = ImageDraw.Draw(img)
        for y, row in enumerate(snap):
            for x, (chdata, fg, bg, is_bold, rev) in enumerate(row):
                f = color(fg, FG)
                b = color(bg, BG)
                if rev:
                    f, b = b, f
                px, py = pad + x * cw, pad + y * ch
                if b != BG:
                    d.rectangle([px, py, px + cw, py + ch], fill=b)
                if chdata not in (" ", ""):
                    d.text((px, py), chdata, font=bold if is_bold else font, fill=f)
        if dump:
            img.save(os.path.join(dump, f"{i:03d}.png"))
        images.append(img.quantize(colors=128, method=Image.MEDIANCUT))
        durations.append(max(40, int(dur * 1000)))
    images[0].save(out, save_all=True, append_images=images[1:],
                   duration=durations, loop=0, optimize=True, disposal=2)
    print(f"{out}: {len(images)} frames, {W}x{H}, "
          f"{sum(durations)/1000:.1f}s, {os.path.getsize(out)/1024:.0f} KiB")


render(run(), OUT)
