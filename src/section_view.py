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

@GtkTemplate(ui='/nl/g4d/Girens/section_view.ui')
class SectionView(Gtk.Box):
    __gtype_name__ = 'section_view'

    __gsignals__ = {
        'view-show-wanted': (GObject.SignalFlags.RUN_FIRST, None, (str,)),
        'view-artist-wanted': (GObject.SignalFlags.RUN_FIRST, None, (str,)),
    }

    _title_label = GtkTemplate.Child()
    _section_flow = GtkTemplate.Child()

    _show_more_button = GtkTemplate.Child()
    _filter_box = GtkTemplate.Child()
    _section_controll_box = GtkTemplate.Child()
    _play_button = GtkTemplate.Child()
    _shuffle_button = GtkTemplate.Child()
    _order_button = GtkTemplate.Child()
    _order_image = GtkTemplate.Child()

    _sort_active = None
    _sort_value_active = None
    _load_spinner = GtkTemplate.Child()

    _timout = None
    _add_items_to_view = 0
    _add_items_first = False
    _cover_width = 200

    _section_key = None

    def __init__(self, plex, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._plex = plex
        self._plex.connect("section-item-retrieved", self.__on_section_items_retrieved)
        self._plex.connect("playlists-retrieved", self.__on_playlists_retrieved)
        self._show_more_button.connect("clicked", self.__show_more_clicked)
        self._filter_box.connect("changed", self.__filter_changed)

        self._play_button.connect("clicked", self.__on_play_button_clicked)
        self._shuffle_button.connect("clicked", self.__on_shuffle_button_clicked)
        self._order_button.connect("clicked", self.__on_order_button_clicked)


    def refresh(self, section, sort=None, sort_value=None):
        if (section.key != self._section_key):
            self._section_key = section.key
            self.__refresh(section, sort=sort, sort_value=sort_value)

    def __refresh(self, section, sort=None, sort_value=None):
        self._section = section

        if (sort == None):
            sort_config = self._plex.get_section_filter(section)
            if (sort_config != None):
                sort = sort_config['sort']
                sort_value = sort_config['sort_value']

        self._sort_active = sort
        self._sort_value_active = sort_value
        if self._sort_value_active == None:
            self._sort_value_active = 'desc'
        self.__set_correct_order_image()

        self._title_label.set_label(self._section.title)

        self.__stop_add_items_timout()
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
        self._section_controll_box.set_visible(True)

        self._load_spinner.set_visible(True)

        thread = threading.Thread(target=self._plex.get_section_items, args=(self._section,),kwargs={'sort':sort, 'sort_value': sort_value})
        thread.daemon = True
        thread.start()

    def show_playlists(self):
        self._sort_active = None

        self._title_label.set_label("Playlists")

        self.__stop_add_items_timout()
        for item in self._section_flow.get_children():
            self._section_flow.remove(item)

        self._show_more_button.set_visible(False)
        self._section_controll_box.set_visible(False)

        self._load_spinner.set_visible(True)

        thread = threading.Thread(target=self._plex.get_playlists)
        thread.daemon = True
        thread.start()

    def __filter_changed(self, combo):
        tree_iter = combo.get_active_iter()
        if tree_iter is not None:
            model = combo.get_model()
            sort_object, sort_string = model[tree_iter][:2]
            if (self._sort_active != sort_string):
                self.refresh(self._section, sort=sort_string, sort_value=self._sort_value_active)

    def __on_order_button_clicked(self, button):
        if self._sort_value_active == 'desc':
            self._sort_value_active = 'asc'
        else:
            self._sort_value_active = 'desc'
        self.refresh(self._section, sort=self._sort_active, sort_value=self._sort_value_active)

    def __set_correct_order_image(self):
        if self._sort_value_active == 'desc':
            self._order_image.set_from_icon_name('go-down-symbolic', 4)
        else:
            self._order_image.set_from_icon_name('go-up-symbolic', 4)

    def __on_playlists_retrieved(self, plex, playlists):
        GLib.idle_add(self.__process_playlists, playlists)

    def __process_playlists(self, playlists):
        self.__process_section_items(playlists)

    def __on_section_items_retrieved(self, plex, items):
        GLib.idle_add(self.__process_section_items, items)

    def __process_section_items(self, items):
        self._items = items
        self.__start_adding_items()
        #self.__show_more_items()
        self._load_spinner.set_visible(False)

    def __stop_add_items_timout(self):
        if self._timout != None:
            GLib.source_remove(self._timout)
            self._timout = None

    def __start_add_items_timout(self):
        self._timout = GLib.timeout_add(50, self.__show_more_items)

    def __show_more_items(self):
        self.__stop_add_items_timout()
        count = 0
        if self._add_items_first == True:
            items_to_add = 15
            self._add_items_first = False
        else:
            items_to_add = 2
        self._add_items_to_view -= items_to_add
        if self._add_items_to_view > 0:
            while count < items_to_add and len(self._items) != 0:
                cover = CoverBox(self._plex, self._items[0], cover_width=self._cover_width)
                cover.connect("view-show-wanted", self.__on_go_to_show_clicked)
                cover.connect("view-artist-wanted", self.__on_go_to_artist_clicked)
                self._section_flow.add(cover)
                self._items.remove(self._items[0])
                count = count + 1

            if (len(self._items) == 0):
                self._show_more_button.set_visible(False)
            else:
                self.__start_add_items_timout()
                self._show_more_button.set_visible(True)

    def __show_more_clicked(self, button):
        self.__start_adding_items()

    def __start_adding_items(self):
        self._add_items_to_view = 50
        self._add_items_first = True
        self.__start_add_items_timout()

    def __on_go_to_show_clicked(self, cover, key):
        self.emit('view-show-wanted', key)

    def __on_go_to_artist_clicked(self, cover, key):
        self.emit('view-artist-wanted', key)

    def __on_play_button_clicked(self, button):
        sort = None
        if self._sort_active is not None:
            sort = self._sort_active + "%3A" + self._sort_value_active
        thread = threading.Thread(target=self._plex.play_item, args=(self._section,),kwargs={'sort':sort})
        thread.daemon = True
        thread.start()

    def __on_shuffle_button_clicked(self, button):
        sort = None
        if self._sort_active is not None:
            sort = self._sort_active + "%3A" + self._sort_value_active
        thread = threading.Thread(target=self._plex.play_item, args=(self._section,),kwargs={'sort':sort, 'shuffle':1})
        thread.daemon = True
        thread.start()
        
    def width_changed(self, width):
        if width < 450:
            self._cover_width = width / 2 - 10
        else:
            self._cover_width = 200
