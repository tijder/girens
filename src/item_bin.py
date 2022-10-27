import gi
from gi.repository import Gtk, GLib, GdkPixbuf, GObject, Gio

class ItemBin(GObject.Object):
    def __init__(self, **kwargs):
        super().__init__(**kwargs)

    def set_item(self, item):
        self._item = item

    def get_item(self):
        return self._item
