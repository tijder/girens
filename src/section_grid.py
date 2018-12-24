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

import threading

@GtkTemplate(ui='/org/gnome/Girens/section_grid.ui')
class SectionGrid(Gtk.Grid):
    __gtype_name__ = 'section_grid'

    __gsignals__ = {
        'section-clicked': (GObject.SignalFlags.RUN_FIRST, None, (object,))
    }

    _title_button = GtkTemplate.Child()

    def __init__(self, data, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        self._data = data

        self._title_button.set_label(data.title)

        self._title_button.connect("clicked", self.__on_section_clicked)

    def __on_section_clicked(self, button):
        self.emit('section-clicked', self._data)
