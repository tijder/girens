# cover_box.py
#
# Copyright 2018 Gerben Droogers
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

from gi.repository import Gtk, GLib, GObject
from .gi_composites import GtkTemplate

from .cover_box import CoverBox

import threading

@GtkTemplate(ui='/org/gnome/Plex/section_view.ui')
class SectionView(Gtk.Box):
    __gtype_name__ = 'section_view'

    __gsignals__ = {
        'view-show-wanted': (GObject.SignalFlags.RUN_FIRST, None, (str,))
    }

    _title_label = GtkTemplate.Child()
    _section_flow = GtkTemplate.Child()

    _show_more_button = GtkTemplate.Child()
    _filter_box = GtkTemplate.Child()

    _sort_active = None

    def __init__(self, plex, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._plex = plex
        self._plex.connect("section-item-retrieved", self.__on_section_items_retrieved)
        self._show_more_button.connect("clicked", self.__show_more_clicked)
        self._filter_box.connect("changed", self.__filter_changed)


    def refresh(self, section, sort=None, sort_value=None):
        self._section = section
        self._sort_active = sort

        self._title_label.set_label(self._section.title)

        for item in self._section_flow.get_children():
            self._section_flow.remove(item)

        self._show_more_button.set_visible(False)


        self._filter_box.clear()
        self._sort_store = Gtk.ListStore(object, str)
        for sort_avaible in self._section.ALLOWED_SORT:
            self._sort_store.append([sort_avaible, str(sort_avaible)])

        self._filter_box.set_model(self._sort_store)
        self._filter_box.set_id_column(1)
        renderer_text = Gtk.CellRendererText()
        self._filter_box.pack_start(renderer_text, True)
        self._filter_box.add_attribute(renderer_text, "text", 1)
        self._filter_box.set_active_id(sort)
        self._filter_box.set_visible(True)

        if (sort != None and sort_value != None):
            sort = sort + ':' + sort_value

        thread = threading.Thread(target=self._plex.get_section_items, args=(self._section,),kwargs={'sort':sort})
        thread.daemon = True
        thread.start()

    def show_playlists(self, playlists):
        self._section = playlists
        self._sort_active = None

        self._title_label.set_label("Playlists")

        for item in self._section_flow.get_children():
            self._section_flow.remove(item)

        self._show_more_button.set_visible(False)
        self._filter_box.set_visible(False)

        self.__process_section_items(playlists)

    def __filter_changed(self, combo):
        tree_iter = combo.get_active_iter()
        if tree_iter is not None:
            model = combo.get_model()
            sort_object, sort_string = model[tree_iter][:2]
            if (self._sort_active != sort_string):
                self.refresh(self._section, sort=sort_string, sort_value="desc")

    def __on_section_items_retrieved(self, plex, items):
        GLib.idle_add(self.__process_section_items, items)

    def __process_section_items(self, items):
        self._items = items
        self.__show_more_items()

    def __show_more_items(self):
        count = 0
        while count < 100 and len(self._items) != 0:
            cover = CoverBox(self._plex, self._items[0])
            cover.connect("view-show-wanted", self.__on_go_to_show_clicked)
            self._section_flow.add(cover)
            self._items.remove(self._items[0])
            count = count + 1

        if (len(self._items) == 0):
            self._show_more_button.set_visible(False)
        else:
            self._show_more_button.set_visible(True)

    def __show_more_clicked(self, button):
        self.__show_more_items()

    def __on_go_to_show_clicked(self, cover, key):
        self.emit('view-show-wanted', key)
