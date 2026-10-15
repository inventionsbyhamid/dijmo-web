def squircle(x, y, w, h, r):
    """iOS/macOS continuous-corner rounded rect (PaintCode reverse-engineered constants)."""
    def P(a,b): return '%.2f %.2f'%(a,b)
    L,T,R,B=x,y,x+w,y+h
    k=[1.52866483,1.08849323,0.86840689,0.63149399,0.07491100,0.37282893,0.16946400]
    a,b,c,d,e,f,g=[v*r for v in k]
    s=[]
    s.append('M'+P(L+a,T))
    s.append('L'+P(R-a,T))
    s.append('C'+P(R-b,T)+' '+P(R-c,T)+' '+P(R-d,T+e))
    s.append('C'+P(R-f,T+g)+' '+P(R-g,T+f)+' '+P(R-e,T+d))
    s.append('C'+P(R,T+c)+' '+P(R,T+b)+' '+P(R,T+a))
    s.append('L'+P(R,B-a))
    s.append('C'+P(R,B-b)+' '+P(R,B-c)+' '+P(R-e,B-d))
    s.append('C'+P(R-g,B-f)+' '+P(R-f,B-g)+' '+P(R-d,B-e))
    s.append('C'+P(R-c,B)+' '+P(R-b,B)+' '+P(R-a,B))
    s.append('L'+P(L+a,B))
    s.append('C'+P(L+b,B)+' '+P(L+c,B)+' '+P(L+d,B-e))
    s.append('C'+P(L+f,B-g)+' '+P(L+g,B-f)+' '+P(L+e,B-d))
    s.append('C'+P(L,B-c)+' '+P(L,B-b)+' '+P(L,B-a))
    s.append('L'+P(L,T+a))
    s.append('C'+P(L,T+b)+' '+P(L,T+c)+' '+P(L+e,T+d))
    s.append('C'+P(L+g,T+f)+' '+P(L+f,T+g)+' '+P(L+d,T+e))
    s.append('C'+P(L+c,T)+' '+P(L+b,T)+' '+P(L+a,T))
    s.append('Z')
    return ''.join(s)
