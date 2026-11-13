import ctypes as c, time, os
from PIL import Image
x=c.CDLL('libX11.so.6'); t=c.CDLL('libXtst.so.6')
x.XOpenDisplay.argtypes=[c.c_char_p]; x.XOpenDisplay.restype=c.c_void_p
d=x.XOpenDisplay(os.environ.get('MICKEY_TEST_DISPLAY', ':93').encode()); assert d
x.XDefaultRootWindow.argtypes=[c.c_void_p]; x.XDefaultRootWindow.restype=c.c_ulong
root=x.XDefaultRootWindow(d)
x.XFlush.argtypes=[c.c_void_p]
x.XStringToKeysym.argtypes=[c.c_char_p]; x.XStringToKeysym.restype=c.c_ulong
x.XKeysymToKeycode.argtypes=[c.c_void_p,c.c_ulong]; x.XKeysymToKeycode.restype=c.c_uint
t.XTestFakeKeyEvent.argtypes=[c.c_void_p,c.c_uint,c.c_int,c.c_ulong]
t.XTestFakeButtonEvent.argtypes=[c.c_void_p,c.c_uint,c.c_int,c.c_ulong]
t.XTestFakeMotionEvent.argtypes=[c.c_void_p,c.c_int,c.c_int,c.c_int,c.c_ulong]
def key(name):
 k=x.XKeysymToKeycode(d,x.XStringToKeysym(name.encode())); t.XTestFakeKeyEvent(d,k,1,0); x.XFlush(d);time.sleep(.035);t.XTestFakeKeyEvent(d,k,0,0);x.XFlush(d);time.sleep(.065)
def click(px,py):
 t.XTestFakeMotionEvent(d,-1,px,py,0); t.XTestFakeButtonEvent(d,1,1,0);x.XFlush(d);time.sleep(.035);t.XTestFakeButtonEvent(d,1,0,0);x.XFlush(d);time.sleep(.065)
class XImage(c.Structure):
 _fields_=[('width',c.c_int),('height',c.c_int),('xoffset',c.c_int),('format',c.c_int),('data',c.c_void_p),('byte_order',c.c_int),('bitmap_unit',c.c_int),('bitmap_bit_order',c.c_int),('bitmap_pad',c.c_int),('depth',c.c_int),('bytes_per_line',c.c_int),('bits_per_pixel',c.c_int),('red_mask',c.c_ulong),('green_mask',c.c_ulong),('blue_mask',c.c_ulong)]
x.XGetImage.argtypes=[c.c_void_p,c.c_ulong,c.c_int,c.c_int,c.c_uint,c.c_uint,c.c_ulong,c.c_int];x.XGetImage.restype=c.POINTER(XImage)
x.XDestroyImage.argtypes=[c.POINTER(XImage)]
def capture(width=1024,height=768):
 p=x.XGetImage(d,root,0,0,width,height,0xffffffff,2); im=p.contents
 result=Image.frombytes('RGB',(im.width,im.height),c.string_at(im.data,im.bytes_per_line*im.height),'raw','BGRX',im.bytes_per_line);x.XDestroyImage(p)
 return result

def shot(path,width=1024,height=768):
 capture(width,height).save(path)

def game_point(px, py, width=960, height=720):
 scale=min((width-32)/320,(height-128)/200)
 return round((width-320*scale)/2+(px+.5)*scale), round(84+(height-128-200*scale)/2+(py+.5)*scale)

def click_game(px, py):
 click(*game_point(px,py))

def shot_game(path):
 bounds=(*game_point(-.5,-.5),*game_point(319.5,199.5))
 canonical=capture().transform((320,200),Image.Transform.EXTENT,bounds,Image.Resampling.NEAREST)
 canonical.resize((960,600),Image.Resampling.NEAREST).save(path)
