"""Match PoolSync's GTK PNG/BMP/empty-text offer in an isolated RDP fixture."""
import ctypes,io,sys
import gi
gi.require_version('Gtk','3.0')
from gi.repository import Gtk
from PIL import Image
Gtk.init([])
image=Image.open(sys.argv[1]);png=open(sys.argv[1],'rb').read();out=io.BytesIO();image.convert('RGB').save(out,format='BMP');bmp=out.getvalue()
lib=ctypes.CDLL('libgtk-3.so.0');gdk=ctypes.CDLL('libgdk-3.so.0')
gdk.gdk_atom_intern.argtypes=[ctypes.c_char_p,ctypes.c_int];gdk.gdk_atom_intern.restype=ctypes.c_void_p
lib.gtk_clipboard_get.argtypes=[ctypes.c_void_p];lib.gtk_clipboard_get.restype=ctypes.c_void_p
lib.gtk_selection_data_get_target.argtypes=[ctypes.c_void_p];lib.gtk_selection_data_get_target.restype=ctypes.c_void_p
lib.gtk_selection_data_set.argtypes=[ctypes.c_void_p,ctypes.c_void_p,ctypes.c_int,ctypes.c_char_p,ctypes.c_int]
lib.gtk_selection_data_set_text.argtypes=[ctypes.c_void_p,ctypes.c_char_p,ctypes.c_int]
class Entry(ctypes.Structure):_fields_=[('target',ctypes.c_char_p),('flags',ctypes.c_uint),('info',ctypes.c_uint)]
callback=ctypes.CFUNCTYPE(None,ctypes.c_void_p,ctypes.c_void_p,ctypes.c_uint,ctypes.c_void_p)
@callback
def get(_clipboard,selection,info,_user):
 if info in (0,1):
  data=png if info==0 else bmp;lib.gtk_selection_data_set(selection,lib.gtk_selection_data_get_target(selection),8,data,len(data))
 else:lib.gtk_selection_data_set_text(selection,b'',0)
items=[(b'image/png',0),(b'UTF8_STRING',2),(b'STRING',2),(b'TEXT',2),(b'text/plain',2),(b'text/plain;charset=utf-8',2),(b'image/bmp',1),(b'image/x-bmp',1)]
if "--image-only" in sys.argv:items=[entry for entry in items if entry[1]!=2]
entries=(Entry*len(items))(*(Entry(name,0,info) for name,info in items))
lib.gtk_clipboard_set_with_data.argtypes=[ctypes.c_void_p,ctypes.POINTER(Entry),ctypes.c_uint,callback,ctypes.c_void_p,ctypes.c_void_p];lib.gtk_clipboard_set_with_data.restype=ctypes.c_int
clip=lib.gtk_clipboard_get(gdk.gdk_atom_intern(b'CLIPBOARD',0));assert lib.gtk_clipboard_set_with_data(clip,entries,len(items),get,None,None)
Gtk.main()
