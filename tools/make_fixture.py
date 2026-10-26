#!/usr/bin/env python3
"""Optional maintainer tool (Pillow only here): generate original synthetic test card."""
from PIL import Image,ImageDraw,ImageFont
from pathlib import Path
image=Image.new('RGB',(320,240),(18,32,24)); d=ImageDraw.Draw(image)
for x in range(0,321,20): d.line((x,0,x,240),fill=(37,59,43))
for y in range(0,241,20): d.line((0,y,320,y),fill=(37,59,43))
d.ellipse((98,52,222,176),outline=(174,219,124),width=2)
d.line((160,38,160,190),fill=(116,154,91)); d.line((84,114,236,114),fill=(116,154,91))
d.ellipse((151,105,169,123),fill=(197,240,149))
d.rectangle((12,12,145,30),fill=(197,240,149)); d.text((18,16),'SYNTHETIC / X-LENS',fill=(17,31,20))
d.rectangle((0,200,320,240),fill=(14,24,18)); d.text((16,210),'TEST SIGNAL  /  320 x 240',fill=(218,235,212)); d.text((16,225),'NO CAMERA CONNECTED',fill=(139,167,146))
image.save(Path(__file__).resolve().parents[1]/'tests/fixtures/synthetic.jpg',quality=75,optimize=True)
