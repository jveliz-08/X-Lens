#!/usr/bin/env python3
"""Render an original synthetic hallway scene for demos and the video recorder (needs Pillow).

    python3 tools/demo_scene.py OUT_DIR

Writes idle.jpg, walk-NNN.jpg (a person carries a box to the door and leaves) and
motion.json with the per-frame motion score (per mille of changed 40x30 cells against an
adaptive background, the same method the ESP32 firmware uses). No camera involved.
"""
import json
import pathlib
import random
import sys
from PIL import Image, ImageDraw, ImageFilter

W,H=320,240
FPS=8

def background():
    img=Image.new('RGB',(W,H),(58,64,74))
    d=ImageDraw.Draw(img)
    d.polygon([(0,170),(W,170),(W,H),(0,H)],fill=(92,80,68))             # floor
    for x in range(-40,W+40,36): d.line([(x,170),(x-30,H)],fill=(82,71,60),width=2)
    d.rectangle([228,62,284,170],fill=(118,84,58));d.rectangle([232,66,280,170],fill=(132,94,64)) # door
    d.ellipse([268,116,274,122],fill=(214,190,120))
    d.rectangle([24,40,104,104],fill=(150,185,214));d.line([(64,40),(64,104)],fill=(70,76,86),width=3) # window
    d.line([(24,72),(104,72)],fill=(70,76,86),width=3);d.rectangle([22,38,106,106],outline=(70,76,86),width=3)
    d.rectangle([150,132,170,170],fill=(120,80,52));d.ellipse([136,96,184,140],fill=(64,120,72))      # plant
    d.rectangle([200,40,212,56],fill=(240,230,190))                                                   # lamp
    return img

def person(d,x,carry):
    """A person close to the camera (about a fifth of the frame)."""
    d.ellipse([x-17,52,x+17,88],fill=(222,184,150))                        # head
    d.rounded_rectangle([x-27,88,x+27,170],radius=10,fill=(196,54,58))    # red jacket
    d.rectangle([x-23,170,x-6,228],fill=(40,48,70));d.rectangle([x+6,170,x+23,228],fill=(40,48,70))
    if carry: d.rectangle([x+14,112,x+58,150],fill=(186,142,88),outline=(140,100,60),width=2)

def frame(base,x=None,carry=False,box=False,seed=0):
    img=base.copy();d=ImageDraw.Draw(img)
    if box: d.rectangle([236,152,262,172],fill=(186,142,88),outline=(140,100,60))
    if x is not None: person(d,x,carry)
    rnd=random.Random(seed) # Mild sensor noise so idle frames are not identical.
    for _ in range(140): d.point((rnd.randrange(W),rnd.randrange(H)),fill=(rnd.randrange(40,110),)*3)
    return img.filter(ImageFilter.GaussianBlur(.4))

def score(img,bg_state):
    small=img.convert('L').resize((40,30))
    px=list(small.tobytes())
    if bg_state[0] is None: bg_state[0]=[float(v) for v in px];return 0
    bg=bg_state[0];shift=sum(p-b for p,b in zip(px,bg))/len(px)
    changed=sum(1 for p,b in zip(px,bg) if abs(p-b-shift)>18)
    bg_state[0]=[b+(p-b)/8 for p,b in zip(px,bg)]
    return round(changed*1000/len(px))

def main():
    out=pathlib.Path(sys.argv[1]);out.mkdir(parents=True,exist_ok=True)
    base=background();state=[None]
    idle=frame(base,seed=1);idle.save(out/'idle.jpg',quality=80)
    for i in range(6): score(frame(base,seed=i),state) # Settle the background on the empty hallway.
    path=[(-30+i*9,True,False) for i in range(28)]+[(222,True,False)]*3+[(222,False,True)]*4+[(222-i*10,False,True) for i in range(28)]
    motion=[]
    for n,(x,carry,box) in enumerate(path):
        img=frame(base,x,carry,box,seed=100+n);img.save(out/f'walk-{n:03d}.jpg',quality=80);motion.append(score(img,state))
    after=frame(base,box=True,seed=999);after.save(out/'after.jpg',quality=80)
    (out/'motion.json').write_text(json.dumps({'fps':FPS,'walk':motion}))

if __name__=='__main__': main()
