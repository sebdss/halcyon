import sys
from PIL import Image
# python3 tools/grid.py sortie.png colonnes img1 img2 ...
out, cols, files = sys.argv[1], int(sys.argv[2]), sys.argv[3:]
ims = [Image.open(f) for f in files]
w, h = 960, int(960 * ims[0].height / ims[0].width)
rows = (len(ims) + cols - 1) // cols
g = Image.new('RGB', (cols * (w + 8), rows * (h + 8)), (255, 0, 255))
for i, im in enumerate(ims):
    g.paste(im.resize((w, h)), ((i % cols) * (w + 8), (i // cols) * (h + 8)))
g.save(out)
