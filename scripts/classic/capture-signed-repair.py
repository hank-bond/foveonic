"""Generate bounded signed repair fixtures from an explicitly supplied reference implementation library."""
import argparse
import ctypes as c
import hashlib
import json
from pathlib import Path

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--library',type=Path,required=True)
p.add_argument('--output',type=Path,required=True)
a=p.parse_args()
pin='99d6d9360f605f0eb83188e8d2acb81cb97d6866df5811ffd8cad9cefb0cb8e1'
assert hashlib.sha256(a.library.read_bytes()).hexdigest()==pin
lib=c.CDLL(str(a.library))
class Image(c.Structure):
    _fields_=[('reserved',c.c_void_p),('pixels',c.c_void_p),('width',c.c_uint32),('height',c.c_uint32),('row_bytes',c.c_int32)]
class Point(c.Structure):
    _fields_=[('x',c.c_int32),('y',c.c_int32)]
repair=getattr(lib,'_ZN15FPixelProc2_F1316ReplaceBadPixelsEP19FPixmapTemplate_F13I17FPixelClass48SRGBEPKNS_13FBadPixel_F13EjPK6FPoint')
repair.argtypes=[c.POINTER(Image),c.POINTER(c.c_uint32),c.c_uint32,c.POINTER(Point)]
functions=[getattr(lib,'_ZN15FPixelProc2_F1312DespeckleRowEPK17FPixelClass48SRGBPS0_S2_j'),getattr(lib,'_ZN15FPixelProc2_F1315DespeckleRowOldEPK17FPixelClass48SRGBPS0_S2_j')]
for f in functions:f.argtypes=[c.c_void_p,c.c_void_p,c.c_void_p,c.c_uint32]
cases=[]
for w,h in [(19,11),(269,7)]:
 values=[((i*1103515245+(i//3)*374761393)&0xffffffff)%20003-10001 for i in range(w*h*3)]
 for mode in range(3):
  data=(c.c_int16*len(values))(*values)
  records=[(4<<20)|(5<<8)|255,(4<<20)|(6<<8)|255,(3<<20)|(4<<8)|37,(1<<20)|(3<<8)|255]
  if mode==0:
   image=Image(None,c.addressof(data),w,h,w*6);packed=(c.c_uint32*len(records))(*records);point=Point(0,0)
   repair(c.byref(image),packed,len(records),c.byref(point))
  else:
   for y in range(1,h-1):functions[mode-1](c.addressof(data)+(y-1)*w*6,c.addressof(data)+y*w*6,c.addressof(data)+(y+1)*w*6,w)
  cases.append(dict(width=w,height=h,input=values,output=list(data),mode=mode,records=records))
a.output.write_text(json.dumps(dict(reference_sha256=pin,cases=cases),separators=(',',':'))+'\n')
