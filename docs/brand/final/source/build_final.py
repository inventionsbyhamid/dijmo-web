"""Dijmo's final logo (the Caret direction): brand files, app icons for the
desktop app, and the website's favicon, touch icon, OG card and header SVG.

    python3 docs/brand/final/source/build_final.py        (from the repo root)

The geometry comes from docs/brand/concepts/source/build.py (class Caret), so
the concept and the final files can never drift apart. Needs fontTools (with
brotli), Pillow, rsvg-convert and, for the .icns, macOS's iconutil."""
import io, os, re, shutil, struct, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, '..', '..', '..', '..'))
sys.path.insert(0, os.path.join(ROOT, 'docs', 'brand', 'concepts', 'source'))
import build as concept                                  # noqa: E402
from build import Caret, face, stem, bounds, to_d, set_word, rr, squircle  # noqa: E402
from clip import c_shape, contours_to_d                  # noqa: E402
from PIL import Image                                    # noqa: E402

FINAL = os.path.join(ROOT, 'docs', 'brand', 'final')
ICONS = os.path.join(FINAL, 'app-icons')
SITE_IMG = os.path.join(ROOT, 'assets', 'img')

POR, INK, RED, BLK, POR_D, WHITE = '#F2F3F1', '#17181A', '#E0301A', '#000000', '#F2F2EF', '#FFFFFF'
TMP = tempfile.mkdtemp(prefix='dijmo-final-')


def num(v, nd=2):
    s = '%.*f' % (nd, v)
    s = s.rstrip('0').rstrip('.') if '.' in s else s
    return '0' if s in ('-0', '') else s


def squeeze(d, nd):
    """re-round every number in a path"""
    return re.sub(r'-?\d+(?:\.\d+)?', lambda m: num(float(m.group()), nd), d)


# --------------------------------------------------------------------------- geometry (font units, y up)
C = Caret()
F = face(C.W)
BOWL, OB, S, CX = C.d_parts(F)
TOP, BOT = C.TOP, C.BOTTOM
RSB = F.adv('d')[0] - bounds(F.contours('d'))[2]
SHIFT = CX + S + RSB + 30
LETTERS, _ = set_word(F, 'ijmo', kern={'ij': -10, 'jm': 10, 'mo': -6}, track=-14)
LAST = LETTERS[-1]
WORD_X1 = SHIFT + LAST[2] + bounds(LAST[1])[2]
WORD_TOP = max(TOP, 1562)            # the tittles of i and j
WORD_BOT = -390                       # the j's descender
MARK_BB = (OB[0], -BOT, CX + S, TOP)  # x0, y_min(as positive dip), x1, top  -> handled below


def mark_paths(s, ox, oy, nd=2):
    """bowl and caret, scaled by s, with font point (ox, oy) at the origin"""
    bowl = squeeze(contours_to_d(BOWL, s, -ox * s, oy * s), nd)
    x = (CX - ox) * s; y = (oy - TOP) * s; w = S * s; h = (TOP - BOT) * s
    caret = 'M%s %sh%sv%sh-%sz' % (num(x, nd), num(y, nd), num(w, nd), num(h, nd), num(w, nd))
    return bowl, caret


def letters_path(s, ox, oy, nd=2):
    return squeeze(''.join(to_d(c, s, (x + SHIFT - ox) * s, oy * s) for ch, c, x in LETTERS), nd)


def svg(w, h, body, title, nd=2, extra=''):
    return ('<svg xmlns="http://www.w3.org/2000/svg" width="%s" height="%s" viewBox="0 0 %s %s"%s>'
            '<title>%s</title>%s</svg>\n' % (num(w, nd), num(h, nd), num(w, nd), num(h, nd), extra, title, body))


def mark_svg(fg, ac, height=128):
    s = height / (TOP - BOT)
    w = (CX + S - OB[0]) * s
    bowl, caret = mark_paths(s, OB[0], TOP)
    return svg(w, height, '<path fill="%s" d="%s"/><path fill="%s" d="%s"/>' % (fg, bowl, ac, caret), 'Dijmo')


def wordmark_body(fg, ac, s, ox, oy, nd=2):
    bowl, caret = mark_paths(s, ox, oy, nd)
    return '<path fill="%s" d="%s%s"/><path fill="%s" d="%s"/>' % (fg, bowl, letters_path(s, ox, oy, nd), ac, caret)


def wordmark_dims(height):
    s = height / (WORD_TOP - WORD_BOT)
    return s, (WORD_X1 - OB[0]) * s


def wordmark_svg(fg, ac, height=64):
    s, w = wordmark_dims(height)
    return svg(w, height, wordmark_body(fg, ac, s, OB[0], WORD_TOP), 'dijmo')


def tile(x, y, size, ground, fg, ac, knock=False):
    """the app icon, flat: continuous-corner tile with the mark at the icon's proportions"""
    k = size / 824
    body = squircle(x, y, size, size, 185.4 * k)
    h = 566 * k
    s = h / (TOP - BOT)
    mw = (CX + S - OB[0]) * s
    mx = x + size / 2 + 4 * k - mw / 2
    my = y + size / 2 + 10 * k - h / 2
    bowl, caret = mark_paths(s, OB[0], TOP)
    tr = 'translate(%s %s)' % (num(mx), num(my))
    if knock:   # one colour: the mark is cut out of the tile
        return '<path fill="%s" fill-rule="evenodd" d="%s%s"/>' % (fg, body, shift_path(bowl + caret, mx, my))
    return ('<path fill="%s" d="%s"/><g transform="%s"><path fill="%s" d="%s"/><path fill="%s" d="%s"/></g>'
            % (ground, body, tr, fg, bowl, ac, caret))


def shift_path(d, dx, dy):
    """translate an absolute path made of M/L/Q/C/Z and relative h/v"""
    out = []; i = 0
    toks = re.findall(r'[A-Za-z]|-?\d+(?:\.\d+)?', d)
    cmd = None; coord = 0
    for t in toks:
        if t.isalpha():
            cmd = t; coord = 0; out.append(t); continue
        v = float(t)
        if cmd in 'MLQCT':
            v += dx if coord % 2 == 0 else dy
        coord += 1
        out.append(num(v))
    return re.sub(r' (?=[A-Za-z])|(?<=[A-Za-z]) ', '', ' '.join(out))


def lockup_svg(theme, height=96):
    """tile + wordmark. On a light ground the tile is the Stage icon, on dark the Porcelain one."""
    t = height
    ws, ww = wordmark_dims(t * 0.62)
    gap = t * 0.30
    W = t + gap + ww
    if theme == 'light':
        body = tile(0, 0, t, BLK, POR_D, RED)
        fg, ac = INK, RED
    elif theme == 'dark':
        body = tile(0, 0, t, POR, INK, RED)
        fg, ac = POR_D, RED
    else:
        col = INK if theme == 'mono' else WHITE
        body = tile(0, 0, t, None, col, None, knock=True)
        fg = ac = col
    # wordmark: its x-height band centred on the tile
    base_y = t / 2 + 620 * ws              # between centring the x-height and the ascender block
    oy = base_y / ws                       # svg Y = (oy - y) * ws puts the baseline at base_y
    body += '<g transform="translate(%s 0)">%s</g>' % (num(t + gap), wordmark_body(fg, ac, ws, OB[0], oy))
    return svg(W, t, body, 'Dijmo')


# --------------------------------------------------------------------------- rendering helpers
def rsvg(svg_text, out, w, h=None):
    p = os.path.join(TMP, 'x.svg')
    open(p, 'w').write(svg_text)
    cmd = ['nice', '-n', '19', 'rsvg-convert', '-w', str(w)] + (['-h', str(h)] if h else []) + [p, '-o', out]
    subprocess.run(cmd, check=True)


def png(svg_text, w, h=None):
    out = os.path.join(TMP, 'r.png')
    rsvg(svg_text, out, w, h)
    return Image.open(out).convert('RGBA')


def save(im, path, flatten=None):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    if flatten:
        bg = Image.new('RGBA', im.size, flatten); bg.alpha_composite(im); im = bg
    im.save(path, optimize=True)


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    open(path, 'w').write(text)


# --------------------------------------------------------------------------- icon variants
MAC_LIGHT = concept.icon_svg(C, 'light')          # 1024 grid, Apple squircle, shadow
MAC_DARK = concept.icon_svg(C, 'dark')
FAVICON = concept.favicon_svg(C)                   # 32 grid, whole pixels at 16 px


def full_bleed(size=1024, mark_h=0.62):
    """iOS / touch icon: the system applies the mask, so the ground fills the square"""
    h = size * mark_h
    s = h / (TOP - BOT)
    mw = (CX + S - OB[0]) * s
    mx = size / 2 + size * 0.004 - mw / 2
    my = size / 2 + size * 0.01 - h / 2
    bowl, caret = mark_paths(s, OB[0], TOP)
    body = ('<defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#F7F8F6"/>'
            '<stop offset="1" stop-color="#EBECE9"/></linearGradient></defs>'
            '<rect width="%d" height="%d" fill="url(#g)"/><g transform="translate(%s %s)">'
            '<path fill="%s" d="%s"/><path fill="%s" d="%s"/></g>' % (size, size, num(mx), num(my), INK, bowl, RED, caret))
    return svg(size, size, body, 'Dijmo')


def win_tile(size=256, inset=0.0, round_=False, edge=True):
    """Windows and Android: a porcelain tile that fills its canvas, with a hairline edge for white grounds"""
    m = size * inset
    w = size - 2 * m
    shape = ('<circle cx="%s" cy="%s" r="%s"' % (num(size / 2), num(size / 2), num(w / 2)) if round_
             else '<path d="%s"' % squircle(m, m, w, w, w * 0.225 / 1.0))
    edge_el = ''
    if edge:
        e = size / 256 * 1.5
        edge_el = (('<circle cx="%s" cy="%s" r="%s"' % (num(size / 2), num(size / 2), num(w / 2 - e / 2))) if round_
                   else '<path d="%s"' % squircle(m + e / 2, m + e / 2, w - e, w - e, (w - e) * 0.225))
        edge_el += ' fill="none" stroke="#000" stroke-opacity=".12" stroke-width="%s"/>' % num(e)
    h = w * 0.60
    s = h / (TOP - BOT)
    mw = (CX + S - OB[0]) * s
    mx = size / 2 + w * 0.004 - mw / 2
    my = size / 2 + w * 0.012 - h / 2
    bowl, caret = mark_paths(s, OB[0], TOP)
    body = (shape + ' fill="%s"/>' % POR + edge_el
            + '<g transform="translate(%s %s)"><path fill="%s" d="%s"/><path fill="%s" d="%s"/></g>'
            % (num(mx), num(my), INK, bowl, RED, caret))
    return svg(size, size, body, 'Dijmo')


def adaptive_foreground(size=432):
    """Android adaptive icon foreground: 108 dp canvas, the mark inside the 66 dp safe circle"""
    h = size * 0.46
    s = h / (TOP - BOT)
    mw = (CX + S - OB[0]) * s
    mx = size / 2 - mw / 2 + size * 0.004
    my = size / 2 - h / 2 + size * 0.01
    bowl, caret = mark_paths(s, OB[0], TOP)
    return svg(size, size, '<g transform="translate(%s %s)"><path fill="%s" d="%s"/><path fill="%s" d="%s"/></g>'
               % (num(mx), num(my), INK, bowl, RED, caret), 'Dijmo')


def ico(path, images):
    """multi-size .ico with PNG-compressed entries (Vista and later)"""
    blobs = []
    for im in images:
        b = io.BytesIO(); im.save(b, 'PNG', optimize=True); blobs.append((im.size[0], b.getvalue()))
    head = struct.pack('<HHH', 0, 1, len(blobs))
    off = 6 + 16 * len(blobs); entries = b''; data = b''
    for size, blob in blobs:
        entries += struct.pack('<BBBBHHII', size % 256, size % 256, 0, 0, 1, 32, len(blob), off + len(data))
        data += blob
    open(path, 'wb').write(head + entries + data)


def build_app_icons():
    out = os.path.join(ICONS, 'icons')
    if os.path.isdir(ICONS):
        shutil.rmtree(ICONS)
    os.makedirs(out)
    write(os.path.join(ICONS, 'icon.svg'), MAC_LIGHT)
    # macOS and Linux PNGs (Tauri's bundle uses icons/icon.png): the Apple squircle with its margin
    for name, px in (('32x32.png', 32), ('64x64.png', 64), ('128x128.png', 128), ('128x128@2x.png', 256), ('icon.png', 512)):
        save(png(MAC_LIGHT, px), os.path.join(out, name))
    # .icns from an .iconset
    iconset = os.path.join(TMP, 'icon.iconset'); os.makedirs(iconset)
    for base in (16, 32, 128, 256, 512):
        save(png(MAC_LIGHT, base), os.path.join(iconset, 'icon_%dx%d.png' % (base, base)))
        save(png(MAC_LIGHT, base * 2), os.path.join(iconset, 'icon_%dx%d@2x.png' % (base, base)))
    subprocess.run(['iconutil', '-c', 'icns', iconset, '-o', os.path.join(out, 'icon.icns')], check=True)
    # Windows .ico: 16 and 32 from the pixel-fitted favicon, larger sizes from the tile
    ico_imgs = []
    for px in (16, 24, 32, 48, 64, 256):
        ico_imgs.append(png(FAVICON, px) if px <= 32 else png(win_tile(256, inset=0.03), px))
    ico(os.path.join(out, 'icon.ico'), ico_imgs)
    # Windows Store tiles
    for px in (30, 44, 71, 89, 107, 142, 150, 284, 310):
        src = FAVICON if px <= 44 else win_tile(256, inset=0.03)
        save(png(src, px), os.path.join(out, 'Square%dx%dLogo.png' % (px, px)))
    save(png(FAVICON, 50), os.path.join(out, 'StoreLogo.png'))
    # Android
    dens = {'mdpi': 1, 'hdpi': 1.5, 'xhdpi': 2, 'xxhdpi': 3, 'xxxhdpi': 4}
    for d, k in dens.items():
        base = os.path.join(out, 'android', 'mipmap-' + d)
        save(png(win_tile(192, inset=1 / 12), round(48 * k)), os.path.join(base, 'ic_launcher.png'))
        save(png(win_tile(192, inset=1 / 12, round_=True), round(48 * k)), os.path.join(base, 'ic_launcher_round.png'))
        save(png(adaptive_foreground(), round(108 * k)), os.path.join(base, 'ic_launcher_foreground.png'))
    write(os.path.join(out, 'android', 'mipmap-anydpi-v26', 'ic_launcher.xml'),
          '<?xml version="1.0" encoding="utf-8"?>\n<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">\n'
          '  <foreground android:drawable="@mipmap/ic_launcher_foreground"/>\n'
          '  <background android:drawable="@color/ic_launcher_background"/>\n</adaptive-icon>')
    write(os.path.join(out, 'android', 'values', 'ic_launcher_background.xml'),
          '<?xml version="1.0" encoding="utf-8"?>\n<resources>\n  <color name="ic_launcher_background">#F2F3F1</color>\n</resources>')
    # iOS: full bleed, flattened (the system masks it, and App Store icons may not be transparent)
    ios = os.path.join(out, 'ios')
    fb = full_bleed()
    for name, px in (('AppIcon-20x20@1x', 20), ('AppIcon-20x20@2x', 40), ('AppIcon-20x20@2x-1', 40), ('AppIcon-20x20@3x', 60),
                     ('AppIcon-29x29@1x', 29), ('AppIcon-29x29@2x', 58), ('AppIcon-29x29@2x-1', 58), ('AppIcon-29x29@3x', 87),
                     ('AppIcon-40x40@1x', 40), ('AppIcon-40x40@2x', 80), ('AppIcon-40x40@2x-1', 80), ('AppIcon-40x40@3x', 120),
                     ('AppIcon-512@2x', 1024), ('AppIcon-60x60@2x', 120), ('AppIcon-60x60@3x', 180),
                     ('AppIcon-76x76@1x', 76), ('AppIcon-76x76@2x', 152), ('AppIcon-83.5x83.5@2x', 167)):
        save(png(fb, px), os.path.join(ios, name + '.png'), flatten=(242, 243, 241, 255))


# --------------------------------------------------------------------------- brand files
def build_brand():
    for th, fg, ac in (('light', INK, RED), ('dark', POR_D, RED), ('mono', INK, INK), ('mono-white', WHITE, WHITE)):
        write(os.path.join(FINAL, 'mark-%s.svg' % th), mark_svg(fg, ac))
        write(os.path.join(FINAL, 'wordmark-%s.svg' % th), wordmark_svg(fg, ac))
        write(os.path.join(FINAL, 'lockup-%s.svg' % th), lockup_svg(th))
    write(os.path.join(FINAL, 'app-icon-light.svg'), MAC_LIGHT)
    write(os.path.join(FINAL, 'app-icon-dark.svg'), MAC_DARK)
    write(os.path.join(FINAL, 'favicon.svg'), FAVICON)
    prev = os.path.join(FINAL, 'png')
    os.makedirs(prev, exist_ok=True)
    for th in ('light', 'dark'):
        for px in (16, 32, 128, 1024):
            save(png(MAC_LIGHT if th == 'light' else MAC_DARK, px), os.path.join(prev, 'app-icon-%s-%d.png' % (th, px)))
    for th in ('light', 'dark', 'mono'):
        for kind in ('mark', 'wordmark', 'lockup'):
            subprocess.run(['nice', '-n', '19', 'rsvg-convert', '-h', '200', os.path.join(FINAL, '%s-%s.svg' % (kind, th)),
                            '-o', os.path.join(prev, '%s-%s.png' % (kind, th))], check=True)


# --------------------------------------------------------------------------- website
def header_svg(height=24):
    """inline in templates/base.html: letters take currentColor, the caret stays red; no style attributes"""
    s, w = wordmark_dims(height)
    body = ('<path fill="currentColor" d="%s%s"/><path fill="%s" d="%s"/>'
            % (mark_paths(s, OB[0], WORD_TOP, 1)[0], letters_path(s, OB[0], WORD_TOP, 1), RED,
               mark_paths(s, OB[0], WORD_TOP, 1)[1]))
    return ('<svg class="mark__logo" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 %s %s" width="%s" height="%s" '
            'aria-hidden="true" focusable="false">%s</svg>' % (num(w, 1), num(height, 1), num(w, 1), num(height, 1), body))


def build_site():
    write(os.path.join(SITE_IMG, 'icon.svg'), FAVICON)
    save(png(full_bleed(), 180), os.path.join(SITE_IMG, 'apple-touch-icon.png'), flatten=(242, 243, 241, 255))
    # OG card: keep the typographic card, swap the old text logo for the lockup
    og_path = os.path.join(SITE_IMG, 'og.png')
    og = Image.open(og_path).convert('RGBA')
    og.paste((242, 243, 241, 255), (56, 44, 244, 110))   # clears the old text logo, or a previous lockup
    p = os.path.join(TMP, 'lock.svg'); open(p, 'w').write(lockup_svg('light', 96))
    out = os.path.join(TMP, 'lock.png')
    subprocess.run(['rsvg-convert', '-h', '56', p, '-o', out], check=True)
    lock = Image.open(out).convert('RGBA')
    og.alpha_composite(lock, (64, 48))
    og.convert('RGB').save(og_path, optimize=True)
    write(os.path.join(FINAL, 'source', 'header.svg.html'), header_svg() + '\n')


if __name__ == '__main__':
    what = sys.argv[1:] or ['brand', 'icons', 'site']
    if 'brand' in what:
        build_brand()
    if 'icons' in what:
        build_app_icons()
    if 'site' in what:
        build_site()
    print('done:', ' '.join(what))
