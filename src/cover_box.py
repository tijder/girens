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

from gi.repository import Gtk, GLib
from .gi_composites import GtkTemplate

@GtkTemplate(ui='/org/gnome/Plex/cover_box.ui')
class CoverBox(Gtk.Box):
    __gtype_name__ = 'cover_box'

    _title_label = GtkTemplate.Child()
    _subtitle_label = GtkTemplate.Child()
    _progress_bar = GtkTemplate.Child()

    def __init__(self, item, **kwargs):
        super().__init__(**kwargs)
        self.init_template()

        if (item.TYPE == 'movie' or item.TYPE == 'episode' and item.viewOffset != 0):
            self._progress_bar.set_fraction(item.viewOffset / item.duration)
            self._progress_bar.set_visible(True)

        if (item.TYPE == 'episode'):
            title = item.grandparentTitle
            subtitle = item.title
        elif (item.TYPE == 'movie'):
            title = item.title
            subtitle = str(item.year)

        self._title_label.set_text(title)
        self._subtitle_label.set_text(subtitle)
