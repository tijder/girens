# list.py
#
# Copyright 2022 Gerben Droogers
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <http://www.gnu.org/licenses/>.

from gi.repository import Gtk, GLib, GObject, GdkPixbuf, Gdk
from .playqueue_item import PlayqueueItem

@Gtk.Template(resource_path='/nl/g4d/Girens/list_playqueue.ui')
class ListPlayqueue(Gtk.GridView):
    __gtype_name__ = 'list_playqueue'

    liststore = Gtk.Template.Child()
    factory = Gtk.Template.Child()

    def __init__(self, **kwargs):
        super().__init__(**kwargs)

    def set_plex(self, plex):
        self._plex = plex

    def add_item(self, item):
        self.liststore.append(item)

    def add_items(self, items):
        self.liststore.splice(0, 0, items, len(items))

    def empty_list(self):
        self.liststore.remove_all()

    @Gtk.Template.Callback()
    def on_setup(self, factory: Gtk.SignalListItemFactory, list_item: Gtk.ListItem) -> None:
        list_item.set_child(PlayqueueItem(self._plex));

    @Gtk.Template.Callback()
    def on_bind(self, factory: Gtk.SignalListItemFactory, list_item: Gtk.ListItem) -> None:
        object = list_item.get_item();
        child = list_item.get_child();
        child.set_item(object.get_item())

