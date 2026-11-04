import numpy as np, sys
w=np.fromfile("q4k.bin",dtype=np.uint8).reshape(17408,20,144)
x=np.fromfile("x.bin",dtype=np.float32,count=5120)
def deq(blk):
    d=np.frombuffer(blk[0:2].tobytes(),np.float16)[0].astype(np.float32); dm=np.frombuffer(blk[2:4].tobytes(),np.float16)[0].astype(np.float32)
    sc=blk[4:16].astype(int); qs=blk[16:144].astype(int); out=np.zeros(256,np.float32)
    for j in range(8):
        if j<4: s=sc[j]&63; m=sc[j+4]&63
        else: s=(sc[j+4]&15)|((sc[j-4]>>6)<<4); m=(sc[j+4]>>4)|((sc[j]>>6)<<4)
        g=j//2; hi=j%2; q=qs[g*32:g*32+32]; nib=(q>>4) if hi else (q&15)
        out[g*64+hi*32:g*64+hi*32+32]=d*s*nib-dm*m
    return out
for r in [0,1,17407]:
    acc=np.float32(0)
    for b in range(20):
        v=deq(w[r,b])
        for l in range(256): acc=np.float32(acc+x[b*256+l]*v[l])
    print(r, float(acc))
