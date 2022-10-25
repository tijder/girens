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

from gettext import gettext as _
from gi.repository import Gtk, GLib, GObject


from .cover_box import CoverBox
from .list import List
from .item_bin import ItemBin

import threading

@Gtk.Template(resource_path='/nl/g4d/Girens/section_view.ui')
class SectionView(Gtk.Box):
    __gtype_name__ = 'section_view'

    _sort_lables = {
        'addedAt': _('Added at'),
        'lastViewedAt': _('Last viewed at'),
        'originallyAvailableAt': _('Originally available at'),
        'titleSort': _('Title'),
        'rating': _('Rating'),
        'unwatched': _('Unwatched'),
        'viewCount': _('View count'),
        'userRating': _('User rating'),
        'mediaHeight': _('Media height'),
        'duration': _('Duration'),
    }

    _title_label = Gtk.Template.Child()
    _section_flow = Gtk.Template.Child()

    _filter_box = Gtk.Template.Child()
    _section_controll_box = Gtk.Template.Child()
    _play_button = Gtk.Template.Child()
    _shuffle_button = Gtk.Template.Child()
    _order_button = Gtk.Template.Child()
    _order_image = Gtk.Template.Child()

    _sort_active = None
    _sort_value_active = None
    _load_spinner = Gtk.Template.Child()

    _timout = None
    _add_items_first = False
    _cover_width = 200
    _container_start = 0
    _container_size = 100

    _section_key = None

    def __init__(self, **kwargs):
        super().__init__(**kwargs)

        self._filter_box.connect("changed", self.__filter_changed)

        self._play_button.connect("clicked", self.__on_play_button_clicked)
        self._shuffle_button.connect("clicked", self.__on_shuffle_button_clicked)
        self._order_button.connect("clicked", self.__on_order_button_clicked)


    def set_plex(self, plex):
        self._plex = plex
        self._plex.connect("section-item-retrieved", self.__on_section_items_retrieved)
        self._plex.connect("playlists-retrieved", self.__on_playlists_retrieved)

        self._section_flow.set_plex(plex)
        self._section_flow.set_grid_mode()


    def refresh(self, section, sort=None, sort_value=None):
        if (section.key != self._section_key or (self._sort_value_active != sort_value and sort_value != None) or (self._sort_active != sort and sort != None)):
            self._section_key = section.key
            self.__refresh(section, sort=sort, sort_value=sort_value)

    def __refresh(self, section, sort=None, sort_value=None):
        self._section = section
        self._container_start = 0

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

        self._section_flow.empty_list()

        self._filter_box.clear()
        self._sort_store = Gtk.ListStore(object, str)
        sort_lable_active = sort
        for sort_avaible in self._section.ALLOWED_SORT:
            lable = sort_avaible
            if sort_avaible in self._sort_lables:
                lable = self._sort_lables[sort_avaible]
                if sort_avaible == sort:
                    sort_lable_active = self._sort_lables[sort_avaible]
            else:
                print(sort_avaible)
            self._sort_store.append([sort_avaible, str(lable)])

        self._filter_box.set_model(self._sort_store)
        self._filter_box.set_id_column(1)
        renderer_text = Gtk.CellRendererText()
        self._filter_box.pack_start(renderer_text, True)
        self._filter_box.add_attribute(renderer_text, "text", 1)
        self._filter_box.set_active_id(sort_lable_active)
        self._section_controll_box.set_visible(True)

        self._load_spinner.set_visible(True)

        self.__start_adding_items()

    def show_playlists(self):
        self._section = None
        self._container_start = 0
        self._section_key = None
        self._sort_active = None

        self._title_label.set_label(_("Playlists"))

        self.__stop_add_items_timout()
        self._section_flow.empty_list()

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
            if (self._sort_active != sort_object):
                self.refresh(self._section, sort=sort_object, sort_value=self._sort_value_active)

    def __on_order_button_clicked(self, button):
        if self._sort_value_active == 'desc':
            new_sort_value = 'asc'
        else:
            new_sort_value = 'desc'
        self.refresh(self._section, sort=self._sort_active, sort_value=new_sort_value)

    def __set_correct_order_image(self):
        if self._sort_value_active == 'desc':
            self._order_image.set_from_icon_name('go-down-symbolic')
        else:
            self._order_image.set_from_icon_name('go-up-symbolic')

    def __on_playlists_retrieved(self, plex, playlists):
        GLib.idle_add(self.__process_playlists, playlists)

    def __process_playlists(self, playlists):
        self.__process_section_items(playlists)

    def __on_section_items_retrieved(self, plex, items):
        GLib.idle_add(self.__process_section_items, items)

    def __process_section_items(self, items):
        self._items = items
        #self.__start_adding_items()
        #self.__show_more_items()
        self._add_items_first = True
        self.__start_add_items_timout()

        self._load_spinner.set_visible(False)

    def __stop_add_items_timout(self):
        if self._timout != None:
            GLib.source_remove(self._timout)
            self._timout = None

    def __start_add_items_timout(self):
        self._timout = GLib.timeout_add(50, self.__show_more_items)

    def __show_more_items(self):
        self.__stop_add_items_timout()
        items = []
        for item in self._items:
            item_bin = ItemBin()
            item_bin.set_item(item)
            items.append(item_bin)
        self._items = []
        self._section_flow.add_items(items)

    @Gtk.Template.Callback()
    def on_scroller_edge_reached(self, widget, position):
        if position == Gtk.PositionType.BOTTOM:
            self.__start_adding_items()

    def __start_adding_items(self):
        thread = threading.Thread(target=self._plex.get_section_items, args=(self._section,),kwargs={'sort':self._sort_active, 'sort_value': self._sort_value_active, 'container_start':self._container_start, 'container_size':self._container_size})
        thread.daemon = True
        thread.start()
        self._container_start = self._container_start + self._container_size

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
        self._section_flow.set_cover_width(self._cover_width)
