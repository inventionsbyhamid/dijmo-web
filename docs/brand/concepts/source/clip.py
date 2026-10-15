"""Clip TrueType (quadratic) contours against a vertical line; build a 'C' from a ring."""
import math
from fontTools.pens.basePen import decomposeQuadraticSegment

def segments(contour):
    segs=[]; cur=None; start=None
    for op,a in contour:
        if op=='moveTo': cur=start=a[0]
        elif op=='lineTo': segs.append(('L',cur,a[0])); cur=a[0]
        elif op=='qCurveTo':
            for c,p in decomposeQuadraticSegment(a):
                segs.append(('Q',cur,c,p)); cur=p
        elif op=='curveTo': raise ValueError('cubic')
        elif op=='closePath':
            if cur!=start: segs.append(('L',cur,start))
    return segs

def qpt(p0,c,p1,t):
    mt=1-t
    return (mt*mt*p0[0]+2*mt*t*c[0]+t*t*p1[0], mt*mt*p0[1]+2*mt*t*c[1]+t*t*p1[1])
def qsplit(p0,c,p1,t):
    a=(p0[0]+(c[0]-p0[0])*t, p0[1]+(c[1]-p0[1])*t)
    b=(c[0]+(p1[0]-c[0])*t, c[1]+(p1[1]-c[1])*t)
    m=(a[0]+(b[0]-a[0])*t, a[1]+(b[1]-a[1])*t)
    return ('Q',p0,a,m),('Q',m,b,p1)
def roots_x(seg,x):
    if seg[0]=='L':
        (x0,y0),(x1,y1)=seg[1],seg[2]
        if x1==x0: return []
        t=(x-x0)/(x1-x0); return [t] if 0<t<1 else []
    p0,c,p1=seg[1],seg[2],seg[3]
    A=p0[0]-2*c[0]+p1[0]; B=2*(c[0]-p0[0]); C=p0[0]-x
    ts=[]
    if abs(A)<1e-9:
        if abs(B)>1e-12: ts=[-C/B]
    else:
        D=B*B-4*A*C
        if D>=0:
            s=math.sqrt(D); ts=[(-B+s)/(2*A),(-B-s)/(2*A)]
    return sorted(t for t in ts if 1e-9<t<1-1e-9)

def split_at_x(segs,x):
    out=[]
    for s in segs:
        ts=roots_x(s,x)
        rest=s; consumed=0.0
        for t in ts:
            tt=(t-consumed)/(1-consumed)
            if rest[0]=='Q':
                a,b=qsplit(rest[1],rest[2],rest[3],tt)
            else:
                p=(rest[1][0]+(rest[2][0]-rest[1][0])*tt, rest[1][1]+(rest[2][1]-rest[1][1])*tt)
                a,b=('L',rest[1],p),('L',p,rest[2])
            # snap split point exactly on x
            a=a[:-1]+((x,a[-1][1]),); b=(b[0],(x,b[1][1]))+b[2:]
            out.append(a); rest=b; consumed=t
        out.append(rest)
    return out

def mid(seg):
    return qpt(seg[1],seg[2],seg[3],0.5) if seg[0]=='Q' else ((seg[1][0]+seg[2][0])/2,(seg[1][1]+seg[2][1])/2)

def chain_left(contour,x):
    """the part of a closed contour with X <= x, as one ordered chain of segments"""
    segs=split_at_x(segments(contour),x)
    inside=[mid(s)[0]<=x for s in segs]
    n=len(segs)
    # rotate so we start at the first inside segment after an outside one
    k=next(i for i in range(n) if inside[i] and not inside[i-1])
    segs=segs[k:]+segs[:k]; inside=inside[k:]+inside[:k]
    chain=[]
    for s,ins in zip(segs,inside):
        if not ins: break
        chain.append(s)
    return chain

def rev(chain):
    out=[]
    for s in reversed(chain):
        out.append(('Q',s[3],s[2],s[1]) if s[0]=='Q' else ('L',s[2],s[1]))
    return out

def c_shape(outer,inner,x):
    """ring (outer, inner contour) clipped to X<=x -> list of closed contours (lists of segs)"""
    co=chain_left(outer,x)
    if max(p[0] for g in segments(inner) for p in (g[1],g[-1]))<=x:
        return [co+[('L',co[-1][-1],co[0][1])], segments(inner)]
    ci=chain_left(inner,x)
    end=co[-1][-1]
    # traverse inner starting from the end nearest `end`
    if abs(ci[0][1][1]-end[1])>abs(ci[-1][-1][1]-end[1]): ci=rev(ci)
    return [co+[('L',end,ci[0][1])]+ci+[('L',ci[-1][-1],co[0][1])]]

def contours_to_d(cs,s=1,dx=0,dy=0): return ''.join(segs_to_d(c,s,dx,dy) for c in cs)
def segs_to_d(segs,s=1,dx=0,dy=0):
    def P(p): 
        X=p[0]*s+dx; Y=-p[1]*s+dy
        return ('%.2f %.2f'%(X,Y))
    d=['M'+P(segs[0][1])]
    for g in segs:
        if g[0]=='L': d.append('L'+P(g[2]))
        else: d.append('Q'+P(g[2])+' '+P(g[3]))
    d.append('Z')
    return ''.join(d)
def segs_bounds(segs):
    xs=[];ys=[]
    for g in segs:
        pts=[g[1],g[-1]]
        if g[0]=='Q':
            for t in (0.25,0.5,0.75): pts.append(qpt(g[1],g[2],g[3],t))
        for p in pts: xs.append(p[0]); ys.append(p[1])
    return min(xs),min(ys),max(xs),max(ys)
