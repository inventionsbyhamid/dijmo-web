from fontTools.ttLib import TTFont
from fontTools.varLib import instancer
from fontTools.pens.recordingPen import DecomposingRecordingPen, RecordingPen
from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.svgPathPen import SVGPathPen
import os
# the site's own copy of Schibsted Grotesk (OFL), four levels up from this folder
FONT=os.path.join(os.path.dirname(os.path.abspath(__file__)),'..','..','..','..','assets','fonts','schibsted-grotesk.woff2')
_cache={}
class Face:
    def __init__(self,w):
        self.f=instancer.instantiateVariableFont(TTFont(FONT),{'wght':w})
        self.gs=self.f.getGlyphSet(); self.cmap=self.f.getBestCmap()
    def contours(self,ch):
        g=self.cmap[ord(ch)]
        rp=DecomposingRecordingPen(self.gs); self.gs[g].draw(rp)
        out=[];cur=[]
        for op,a in rp.value:
            cur.append((op,a))
            if op in('closePath','endPath'): out.append(cur);cur=[]
        return out
    def adv(self,ch): return self.f['hmtx'][self.cmap[ord(ch)]]
def bounds(contour_list):
    bp=BoundsPen(None)
    for c in contour_list:
        r=RecordingPen(); r.value=c; r.replay(bp)
    return bp.bounds
def to_d(contour_list, s, dx, dy, nd=2):
    """font units (y up) -> svg (y down): X = x*s+dx, Y = -y*s+dy"""
    sp=SVGPathPen(None, ntos=lambda v: ('%.*f'%(nd,v)).rstrip('0').rstrip('.') if '.' in ('%.*f'%(nd,v)) else '%.*f'%(nd,v))
    tp=TransformPen(sp,(s,0,0,-s,dx,dy))
    for c in contour_list:
        r=RecordingPen(); r.value=c; r.replay(tp)
    return sp.getCommands()
