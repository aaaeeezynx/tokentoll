from pathlib import Path
from PIL import Image, ImageDraw

out = Path(r"D:\token counter\icon-candidates")
out.mkdir(exist_ok=True)
im = Image.new("RGB", (64, 64), "#132f54")
d = ImageDraw.Draw(im)
d.rounded_rectangle((16, 11, 48, 54), radius=15, fill="#d8eeff")
d.rounded_rectangle((22, 17, 42, 56), radius=9, fill="#132f54")
d.rounded_rectangle((25, 21, 39, 62), radius=7, fill="#132f54")
d.ellipse((30, 33, 51, 54), fill="#66dedb")
d.ellipse((36, 39, 45, 48), fill="#132f54")
im.save(out / "concept-64.png")
print("Rendered 64px gateway and token sketch")
