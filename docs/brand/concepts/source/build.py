"""Dijmo logo concepts: builds every SVG for the four directions from one set of
geometry, then renders PNG proofs with rsvg-convert.

    python3 source/build.py .        (run from docs/brand/concepts)

Needs fontTools (with brotli for woff2), Pillow, and rsvg-convert on PATH.

All letterforms are outlines taken from Schibsted Grotesk (OFL) via fontTools,
so no SVG depends on an installed font."""
import math, os, subprocess, sys, tempfile
from fontlib import Face, to_d, bounds
from clip import c_shape, contours_to_d
from squircle import squircle

OUT = sys.argv[1] if len(sys.argv) > 1 else 'out'

POR = '#F2F3F1'; INK = '#17181A'; RED = '#E0301A'; BLK = '#000000'
POR_D = '#F2F2EF'; QUIET = '#5E6166'; LINE_L = '#D9DBD8'; LINE_D = '#262626'

_faces = {}
def face(w):
    if w not in _faces:
        _faces[w] = Face(w)
    return _faces[w]

def f(v):
    s = '%.2f' % v
    return s.rstrip('0').rstrip('.') if '.' in s else s

def rect(x, y, w, h, fill, rx=0):
    r = ' rx="%s"' % f(rx) if rx else ''
    return '<rect x="%s" y="%s" width="%s" height="%s"%s fill="%s"/>' % (f(x), f(y), f(w), f(h), r, fill)

def circle(cx, cy, r, fill):
    return '<circle cx="%s" cy="%s" r="%s" fill="%s"/>' % (f(cx), f(cy), f(r), fill)

def path(d, fill, rule=None):
    fr = ' fill-rule="%s"' % rule if rule else ''
    return '<path d="%s" fill="%s"%s/>' % (d, fill, fr)

def rr(x, y, w, h, r):
    """rounded rect as a path (so it can sit inside an even-odd compound)"""
    return ('M%s %sH%sA%s %s 0 0 1 %s %sV%sA%s %s 0 0 1 %s %sH%sA%s %s 0 0 1 %s %sV%sA%s %s 0 0 1 %s %sZ' % (
        f(x + r), f(y), f(x + w - r), f(r), f(r), f(x + w), f(y + r), f(y + h - r), f(r), f(r), f(x + w - r), f(y + h),
        f(x + r), f(r), f(r), f(x), f(y + h - r), f(y + r), f(r), f(r), f(x + r), f(y)))

def stem(F):
    b = bounds([F.contours('i')[0]])
    return b[2] - b[0]

# Themes. fg = the ink role, ac = the accent (caret red), gr = the ground it sits on.
THEMES = {
    'light': dict(fg=INK, ac=RED, gr=POR, quiet=QUIET, line=LINE_L, mono=False),
    'dark':  dict(fg=POR_D, ac=RED, gr=BLK, quiet='#8F8F8A', line=LINE_D, mono=False),
    'mono':  dict(fg=INK, ac=INK, gr=None, quiet=INK, line=INK, mono=True),
}

def g(els, x0, y0, x1, y1, box, align='center'):
    """fit local geometry (bbox x0..y1) into box=(bx,by,bw,bh); returns <g>"""
    bx, by, bw, bh = box
    w, h = x1 - x0, y1 - y0
    s = min(bw / w, bh / h)
    tx = bx + (bw - w * s) / 2 - x0 * s
    ty = by + (bh - h * s) / 2 - y0 * s
    return '<g transform="translate(%s %s) scale(%s)">%s</g>' % (f(tx), f(ty), '%.6f' % s, els)

def svg_doc(w, h, body, vb=None, title=None):
    vb = vb or '0 0 %s %s' % (f(w), f(h))
    t = '<title>%s</title>' % title if title else ''
    return ('<svg xmlns="http://www.w3.org/2000/svg" width="%s" height="%s" viewBox="%s">%s%s</svg>\n'
            % (f(w), f(h), vb, t, body))

def tight_svg(els, bb, height, title):
    """standalone logo file: viewBox = geometry bounds, scaled to a given pixel height"""
    x0, y0, x1, y1 = bb
    w = (x1 - x0) / (y1 - y0) * height
    return svg_doc(w, height, g(els, x0, y0, x1, y1, (0, 0, w, height)), title=title)

# --------------------------------------------------------------------------- word setting
def set_word(F, text, kern=None, track=0, skip=()):
    """returns [(ch, contours, x)], advance. kern: {'ij': units}"""
    kern = kern or {}
    x = 0; out = []; prev = None
    for ch in text:
        if prev:
            x += kern.get(prev + ch, 0) + track
        out.append((ch, F.contours(ch), x))
        x += F.adv(ch)[0]
        prev = ch
    return out, x

def word_d(parts, skip=()):
    return ''.join(to_d(c, 1, x, 0) for ch, c, x in parts if ch not in skip)

# =========================================================================== 1. CARET
class Caret:
    key = 'caret'; name = 'Caret'
    W = 520            # weight of the wordmark and of the mark's bowl
    GAP = 0.30         # gap between bowl and caret, in stems
    TOP, BOTTOM = 1500, -150   # caret spans the ascender and dips under the baseline

    def d_parts(self, F):
        o = F.contours('o'); ob = bounds(o); S = stem(F)
        clipx = ob[2] - 1.0 * S
        bowl = c_shape(o[0], o[1], clipx)
        cx = clipx + self.GAP * S
        return bowl, ob, S, cx

    def mark(self, t):
        F = face(self.W)
        bowl, ob, S, cx = self.d_parts(F)
        els = path(contours_to_d(bowl), t['fg']) + rect(cx, -self.TOP, S, self.TOP - self.BOTTOM, t['ac'])
        return els, (ob[0], -self.TOP, cx + S, -self.BOTTOM)

    def wordmark(self, t):
        F = face(self.W)
        bowl, ob, S, cx = self.d_parts(F)
        rsb = F.adv('d')[0] - bounds(F.contours('d'))[2]
        d_adv = cx + S + rsb
        parts, adv = set_word(F, 'ijmo', kern={'ij': -10, 'jm': 10, 'mo': -6}, track=-14)
        els = path(contours_to_d(bowl), t['fg']) + rect(cx, -self.TOP, S, self.TOP - self.BOTTOM, t['ac'])
        shift = d_adv + 30
        els += '<path d="%s" fill="%s"/>' % (''.join(to_d(c, 1, x + shift, 0) for ch, c, x in parts), t['fg'])
        last = parts[-1]
        x1 = shift + last[2] + bounds(last[1])[2]
        return els, (ob[0], -max(self.TOP, 1562), x1, 390)

    # icon: porcelain body, mark optically centred (bowl mass sits low and left)
    icon_grounds = {'light': 'porcelain', 'dark': 'stage'}
    def icon_art(self, t):
        els, bb = self.mark(t)
        x0, y0, x1, y1 = bb
        h = 566; w = (x1 - x0) / (y1 - y0) * h
        cx = 512 + 4; cy = 512 + 10
        return g(els, x0, y0, x1, y1, (cx - w / 2, cy - h / 2, w, h))

    def favicon(self):
        # 32-unit grid, 2 units = 1 px at 16 px. Caret edges sit on whole pixels.
        F = face(700)
        bowl, ob, S, cx = self.d_parts(F)
        top, bot = 1500, -150
        # 2 units = 1 px at 16 px; the caret's four edges sit on whole pixels (x 11-13, y 2-14)
        s = 24 / (top - bot)            # the caret runs 24 of 32 units
        X0 = 22 - cx * s
        Y0 = 4 + top * s
        bowl_d = contours_to_d(bowl, s, X0, Y0)
        body = rr(0, 0, 32, 32, 7.2)
        return (path(body, POR) + path(bowl_d, INK)
                + rect(22, 4, 4, 24, RED))

# =========================================================================== 2. SELECTION
class Selection:
    key = 'selection'; name = 'Selection'
    W = 520

    def hl(self, t):
        if t['mono']:
            return None
        return '#F2C4BC' if t['gr'] in (POR, None) else '#5E1E15'

    def bracket(self, F, x0, x1, top, bot, t, S, K, gap):
        """selection highlight from x0..x1 (font units, y down), handles outside it"""
        els = ''
        hl = self.hl(t)
        if hl:
            els += rect(x0, top, x1 - x0, bot - top, hl)
        els += rect(x0 - gap - S, top, S, bot - top, t['ac']) + circle(x0 - gap - S / 2, top - K / 2, K / 2, t['ac'])
        els += rect(x1 + gap, top, S, bot - top, t['ac']) + circle(x1 + gap + S / 2, bot + K / 2, K / 2, t['ac'])
        bb = (x0 - gap - S / 2 - K / 2, top - K, x1 + gap + S / 2 + K / 2, bot + K)
        return els, bb

    def mark(self, t, weight=600, pad=90):
        F = face(weight); St = stem(F)
        c = F.contours('d'); b = bounds(c)
        x0, x1 = b[0] - pad, b[2] + pad
        top, bot = -1500 - 90, 300
        S = St * 0.42; K = St * 1.30
        gap = St * 0.16 if t['mono'] else 0   # one colour needs a seam; colour abuts, like a real selection
        if t['mono']:
            # one colour: the highlight becomes solid, the d is cut out of it
            sel = rr(x0, top, x1 - x0, bot - top, 0) + to_d(c, 1, 0, 0)
            els = path(sel, t['fg'], 'evenodd')
            br, bb = self.bracket(F, x0, x1, top, bot, dict(t, mono=True), S, K, gap)
            return els + br, bb
        br, bb = self.bracket(F, x0, x1, top, bot, t, S, K, gap)
        hl_el, handles = br.split('/>', 1)[0] + '/>', br.split('/>', 1)[1]
        return hl_el + path(to_d(c, 1, 0, 0), t['fg']) + handles, bb

    def wordmark(self, t):
        F = face(self.W); St = stem(F)
        parts, adv = set_word(F, 'dijmo', kern={'ij': -10, 'jm': 10, 'mo': -6}, track=-14)
        dd = word_d(parts)
        x0 = bounds(parts[0][1])[0] - 70
        last = parts[-1]; x1 = last[2] + bounds(last[1])[2] + 70
        top, bot = -1500 - 70, 390 + 50
        S = St * 0.40; K = St * 1.22
        gap = St * 0.16 if t['mono'] else 0
        if t['mono']:
            br, bb = self.bracket(F, x0, x1, top, bot, t, S, K, gap)
            return path(dd, t['fg']) + br, bb
        br, bb = self.bracket(F, x0, x1, top, bot, t, S, K, gap)
        hl_el, handles = br.split('/>', 1)[0] + '/>', br.split('/>', 1)[1]
        return hl_el + path(dd, t['fg']) + handles, bb

    icon_grounds = {'light': 'porcelain', 'dark': 'stage'}
    def icon_art(self, t):
        els, bb = self.mark(t)
        x0, y0, x1, y1 = bb
        h = 600; w = (x1 - x0) / (y1 - y0) * h
        return g(els, x0, y0, x1, y1, (512 - w / 2, 512 - h / 2, w, h))

    def favicon(self):
        # highlight tile, d, and the two handle knobs at opposite corners
        F = face(700)
        c = F.contours('d'); b = bounds(c)
        s = 21 / 1525
        X0 = 16 - (b[0] + b[2]) / 2 * s
        Y0 = 16.5 + (1500 - 25) / 2 * s
        body = rr(0, 0, 32, 32, 7.2)
        els = path(body, POR) + rect(6, 4, 20, 24, '#F2C4BC')
        els += path(to_d(c, s, X0, Y0), INK)
        els += rect(4, 4, 2, 24, RED) + circle(5, 3.6, 3.4, RED)
        els += rect(26, 4, 2, 24, RED) + circle(27, 28.4, 3.4, RED)
        return els

# =========================================================================== 3. CUT
class Cut:
    key = 'cut'; name = 'Cut'
    POS = 0.382   # slit centre, share of the diameter from the left
    SLIT = 0.068

    def disc(self, cx, cy, R, fill, slit=None):
        slit = slit or self.SLIT
        sc = cx - R + 2 * R * self.POS
        a = sc - R * slit; b = sc + R * slit
        ha = math.sqrt(R * R - (a - cx) ** 2); hb = math.sqrt(R * R - (b - cx) ** 2)
        left = 'M%s %sA%s %s 0 0 0 %s %sZ' % (f(a), f(cy - ha), f(R), f(R), f(a), f(cy + ha))
        right = 'M%s %sA%s %s 0 1 1 %s %sZ' % (f(b), f(cy - hb), f(R), f(R), f(b), f(cy + hb))
        return path(left + right, fill)

    def mark(self, t):
        return self.disc(50, 50, 50, t['ac']), (0, 0, 100, 100)

    def wordmark(self, t):
        F = face(600)
        parts, adv = set_word(F, 'dijm', kern={'ij': -12, 'jm': 6}, track=-16)
        o_lsb = bounds(F.contours('o'))[0]
        D = 1010                        # a filled disc looks larger than a ring, so it sits inside the o's size
        cx = adv - 16 - 4 + o_lsb + D / 2
        els = path(word_d(parts), t['fg']) + self.disc(cx, -540, D / 2, t['ac'], slit=0.085)
        return els, (bounds(parts[0][1])[0], -1579, cx + D / 2, 390)

    icon_grounds = {'light': 'porcelain', 'dark': 'stage'}
    def icon_art(self, t):
        R = 236
        return self.disc(512, 512, R, t['ac'])

    def favicon(self):
        # no tile: the red dot reads on light and dark tabs; slit = 1 px at 16 px
        R = 15
        a, b = 12, 14                    # the slit is exactly pixel 6 at 16 px (centre 0.40 of the dot)
        cx = cy = 16
        ha = math.sqrt(R * R - (a - cx) ** 2); hb = math.sqrt(R * R - (b - cx) ** 2)
        left = 'M%s %sA%s %s 0 0 0 %s %sZ' % (f(a), f(cy - ha), f(R), f(R), f(a), f(cy + ha))
        right = 'M%s %sA%s %s 0 1 1 %s %sZ' % (f(b), f(cy - hb), f(R), f(R), f(b), f(cy + hb))
        return path(left + right, RED)

# =========================================================================== 4. THOCK
class Thock:
    key = 'thock'; name = 'Thock'
    # keycap seen from above: thin skirt at the back and sides, a deep one at the front
    W, H, RAD = 100, 100, 24
    SIDE, TOP, BOT = 8, 5, 25

    def face_box(self, W=None, H=None):
        W = W or self.W; H = H or self.H
        k = W / 100
        return self.SIDE * k, self.TOP * k, W - 2 * self.SIDE * k, H - (self.TOP + self.BOT) * k

    def legend(self, fx, fy, fw, fh, fill):
        # the caret, set top-left like the legends on old Apple keycaps
        return rect(fx + fw * 0.18, fy + fh * 0.17, fw * 0.072, fh * 0.42, fill)

    def mark(self, t):
        W, H = self.W, self.H
        fx, fy, fw, fh = self.face_box()
        outer = rr(0, 0, W, H, self.RAD); inner = rr(fx, fy, fw, fh, self.RAD - 6)
        if t['gr'] == BLK:
            # on Stage the key keeps its porcelain face, so it stays a key and not a screen
            els = path(outer + inner, t['quiet'], 'evenodd') + path(inner, t['fg'])
        else:
            els = path(outer + inner, t['fg'], 'evenodd')
        els += self.legend(fx, fy, fw, fh, t['ac'])
        return els, (0, 0, W, H)

    def wordmark(self, t):
        F = face(620)
        parts, adv = set_word(F, 'dijmo', kern={'ij': -8, 'jm': 10, 'mo': -6}, track=-12)
        d = ''
        for ch, cs, x in parts:
            if ch in 'ij':
                body, dot = cs[0], cs[1]
                b = bounds([dot])
                s = (b[2] - b[0]) * 0.94
                cxm = (b[0] + b[2]) / 2 + x; cym = -(b[1] + b[3]) / 2
                d += to_d([body], 1, x, 0) + rr(cxm - s / 2, cym - s / 2, s, s, s * 0.3)
            else:
                d += to_d(cs, 1, x, 0)
        last = parts[-1]
        return path(d, t['fg']), (bounds(parts[0][1])[0], -1579, last[2] + bounds(last[1])[2], 390)

    icon_grounds = {'light': 'keycap', 'dark': 'keycap-dark'}
    def icon_art(self, t):
        return ''   # the whole icon is the key, drawn in icon()

    def favicon(self):
        # on whole pixels at 16 px: 1 px back and sides, 4 px front skirt, a 1 x 4 px legend
        fx, fy, fw, fh = 2, 2, 28, 20
        outer = rr(0, 0, 32, 32, 7.6); inner = rr(fx, fy, fw, fh, 5.6)
        return (path(outer + inner, INK, 'evenodd') + path(inner, POR)
                + rect(8, 6, 2, 8, RED))

DIRS = [Caret(), Selection(), Cut(), Thock()]

# --------------------------------------------------------------------------- icon shell
BODY = squircle(100, 100, 824, 824, 185.4)

def icon_svg(d, theme):
    t = THEMES[theme]
    kind = d.icon_grounds[theme]
    defs = ('<defs><filter id="sh" x="-10%" y="-10%" width="120%" height="125%">'
            '<feGaussianBlur stdDeviation="10"/></filter>')
    if kind in ('porcelain', 'stage'):
        top, bot = ('#F7F8F6', '#EBECE9') if kind == 'porcelain' else ('#161616', '#000000')
        defs += ('<linearGradient id="body" x1="0" y1="0" x2="0" y2="1">'
                 '<stop offset="0" stop-color="%s"/><stop offset="1" stop-color="%s"/></linearGradient></defs>' % (top, bot))
        body = ('<path d="%s" fill="#000" opacity=".28" filter="url(#sh)" transform="translate(0 10)"/>'
                '<path d="%s" fill="url(#body)"/>' % (BODY, BODY))
        rim = ''
        if kind == 'stage':
            rim = '<path d="%s" fill="none" stroke="#FFFFFF" stroke-opacity=".10" stroke-width="2"/>' % squircle(101, 101, 822, 822, 184.4)
        art_t = dict(THEMES['light' if kind == 'porcelain' else 'dark'])
        return svg_doc(1024, 1024, defs + body + rim + d.icon_art(art_t), title='Dijmo')
    # Thock: the icon is the keycap itself
    # Light from above. The top face is a shallow concave dish: shaded under its
    # back rim, brighter towards the front lip. The deep front skirt is the thock.
    dark = kind == 'keycap-dark'
    if not dark:
        skirt = ('#E9EAE7', '#C9CBC7'); face_g = ('#ECEDEA', '#FAFAF9'); rim_op = '.95'; inner_op = '.10'; lift_op = '.18'
    else:
        skirt = ('#2C2D2F', '#101011'); face_g = ('#2A2B2D', '#38393C'); rim_op = '.16'; inner_op = '.45'; lift_op = '.6'
    fx, fy, fw, fh = 100 + 824 * 0.075, 100 + 824 * 0.045, 824 * 0.85, 824 * 0.72
    face = squircle(fx, fy, fw, fh, 150)
    defs += ('<linearGradient id="skirt" x1="0" y1="0" x2="0" y2="1"><stop offset=".55" stop-color="%s"/>'
             '<stop offset="1" stop-color="%s"/></linearGradient>'
             '<linearGradient id="face" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="%s"/>'
             '<stop offset="1" stop-color="%s"/></linearGradient>'
             '<clipPath id="fc"><path d="%s"/></clipPath>'
             '<filter id="lift" x="-10%%" y="-10%%" width="120%%" height="130%%"><feGaussianBlur stdDeviation="14"/></filter>'
             '<filter id="soft" x="-10%%" y="-10%%" width="120%%" height="120%%"><feGaussianBlur stdDeviation="18"/></filter>'
             '</defs>' % (skirt[0], skirt[1], face_g[0], face_g[1], face))
    body = ('<path d="%s" fill="#000" opacity=".28" filter="url(#sh)" transform="translate(0 10)"/>' % BODY
            + '<path d="%s" fill="url(#skirt)"/>' % BODY
            + '<path d="%s" fill="#000" opacity="%s" filter="url(#lift)" transform="translate(0 24)"/>' % (face, lift_op)
            + '<path d="%s" fill="url(#face)"/>' % face
            # the dish: a soft shadow inside the back rim
            + '<g clip-path="url(#fc)"><path d="%s" fill="none" stroke="#000" stroke-opacity="%s" stroke-width="44" '
              'filter="url(#soft)" transform="translate(0 26)"/></g>' % (face, inner_op)
            # the front lip catches the light
            + '<path d="%s" fill="none" stroke="#FFF" stroke-opacity="%s" stroke-width="3"/>' % (
                squircle(fx + 1.5, fy + 1.5, fw - 3, fh - 3, 148.5), rim_op))
    if dark:
        body += '<path d="%s" fill="none" stroke="#FFFFFF" stroke-opacity=".12" stroke-width="2"/>' % squircle(101, 101, 822, 822, 184.4)
    leg = rect(fx + fw * 0.17, fy + fh * 0.16, fw * 0.064, fh * 0.40, RED)
    return svg_doc(1024, 1024, defs + body + leg, title='Dijmo')

def favicon_svg(d):
    return svg_doc(32, 32, d.favicon(), title='Dijmo')

# --------------------------------------------------------------------------- write
def write(p, s):
    os.makedirs(os.path.dirname(p), exist_ok=True)
    open(p, 'w').write(s)

def render(svg, png, w, h=None):
    os.makedirs(os.path.dirname(png), exist_ok=True)
    cmd = ['nice', '-n', '19', 'rsvg-convert', '-w', str(w)]
    if h:
        cmd += ['-h', str(h)]
    subprocess.run(cmd + [svg, '-o', png], check=True)

def main():
    for d in DIRS:
        base = os.path.join(OUT, d.key)
        for th in ('light', 'dark', 'mono'):
            t = THEMES[th]
            els, bb = d.mark(t)
            write(f'{base}/mark-{th}.svg', tight_svg(els, bb, 512, 'Dijmo mark'))
            els, bb = d.wordmark(t)
            write(f'{base}/wordmark-{th}.svg', tight_svg(els, bb, 160, 'dijmo'))
        for th in ('light', 'dark'):
            write(f'{base}/app-icon-{th}.svg', icon_svg(d, th))
        write(f'{base}/favicon.svg', favicon_svg(d))
        # PNG proofs
        for th in ('light', 'dark'):
            for s in (16, 32, 128, 1024):
                render(f'{base}/app-icon-{th}.svg', f'{base}/png/app-icon-{th}-{s}.png', s)
        tmp = tempfile.mkdtemp(prefix='dijmo-mark-')
        for th in ('light', 'dark', 'mono'):
            els, bb = d.mark(THEMES[th])
            sq = svg_doc(100, 100, g(els, *bb, (8, 8, 84, 84)))
            p = os.path.join(tmp, f'{d.key}-{th}.svg'); open(p, 'w').write(sq)
            for s in (16, 32, 128, 1024):
                render(p, f'{base}/png/mark-{th}-{s}.png', s, s)
        for s in (16, 32, 64):
            render(f'{base}/favicon.svg', f'{base}/png/favicon-{s}.png', s)
        for th in ('light', 'dark', 'mono'):
            subprocess.run(['nice', '-n', '19', 'rsvg-convert', '-h', '200', f'{base}/wordmark-{th}.svg',
                            '-o', f'{base}/png/wordmark-{th}.png'], check=True)
        preview(d, base)
    print('built', OUT)

def preview(d, base):
    """one proof sheet per direction: light half, dark half, a one-colour strip, 16 px loupes"""
    from PIL import Image
    P = lambda n: Image.open(f'{base}/png/{n}').convert('RGBA')
    W, H = 1800, 1120
    sheet = Image.new('RGBA', (W, H), (242, 243, 241, 255))
    sheet.paste((0, 0, 0, 255), (900, 0, 1800, 900))
    sheet.paste((255, 255, 255, 255), (0, 900, 1800, 1120))

    def put(im, x, y):
        sheet.alpha_composite(im, (int(x), int(y)))

    def fit_h(im, h):
        return im.resize((round(im.width * h / im.height), h), Image.LANCZOS)

    for i, th in enumerate(('light', 'dark')):
        ox = i * 900
        ground = (242, 243, 241, 255) if th == 'light' else (0, 0, 0, 255)
        put(P(f'app-icon-{th}-1024.png').resize((400, 400), Image.LANCZOS), ox + 30, 30)
        x = ox + 470
        for s in (128, 32, 16):                       # true pixel sizes
            put(P(f'app-icon-{th}-{s}.png'), x, 30 + (128 - s)); x += s + 26
        loupe = Image.new('RGBA', (16, 16), ground); loupe.alpha_composite(P(f'app-icon-{th}-16.png'))
        put(loupe.resize((96, 96), Image.NEAREST), ox + 470, 200)
        fav = Image.new('RGBA', (16, 16), ground); fav.alpha_composite(P('favicon-16.png'))
        put(fav.resize((96, 96), Image.NEAREST), ox + 590, 200)
        put(P('favicon-16.png'), ox + 710, 240); put(P('favicon-32.png'), ox + 740, 232)
        put(P(f'mark-{th}-1024.png').resize((260, 260), Image.LANCZOS), ox + 30, 470)
        x = ox + 320
        for s in (128, 32, 16):
            put(P(f'mark-{th}-{s}.png'), x, 470 + (128 - s)); x += s + 26
        put(fit_h(P(f'wordmark-{th}.png'), 110), ox + 320, 640)
    # one colour
    put(P('mark-mono-1024.png').resize((180, 180), Image.LANCZOS), 30, 920)
    x = 240
    for s in (32, 16):
        put(P(f'mark-mono-{s}.png'), x, 1010 - s // 2); x += s + 26
    put(fit_h(P('wordmark-mono.png'), 110), 340, 955)
    sheet.convert('RGB').save(f'{base}/preview.png', optimize=True)

if __name__ == '__main__':
    main()
