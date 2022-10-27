# window.py
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

from gi.repository import Gtk, GLib, GdkPixbuf, Adw


import threading

@Gtk.Template(resource_path='/nl/g4d/Girens/profile_dialog.ui')
class ProfileDialog(Adw.PreferencesWindow):
    __gtype_name__ = 'profile_dialog'

    #_avatar_image = Gtk.Template.Child()

    _logout_button = Gtk.Template.Child()

    _username_value_label = Gtk.Template.Child()
    _email_value_label = Gtk.Template.Child()
    _subscriptionplan_value_label = Gtk.Template.Child()
    _secure_value_label = Gtk.Template.Child()

    def __init__(self, plex, **kwargs):
        super().__init__(**kwargs)
        

        self._logout_button.connect("clicked", self.__on_logout_clicked)

        username = ''
        email = ''
        subscriptionPlan = ''
        secure = ''

        self._plex = plex;

        if (hasattr(self._plex._account, 'username')):
            username = self._plex._account.username
        if (hasattr(self._plex._account, 'email')):
            email = self._plex._account.email
        if (hasattr(self._plex._account, 'subscriptionPlansubscriptionPlan')):
            subscriptionPlan = self._plex._account.subscriptionPlan
        if (hasattr(self._plex._account, 'secure')):
            secure = str(self._plex._account.secure)

        self._username_value_label.set_title(username)
        self._email_value_label.set_title(email)
        self._subscriptionplan_value_label.set_title(subscriptionPlan)
        self._secure_value_label.set_title(secure)

        if (hasattr(self._plex._account, 'thumb')):
            thread = threading.Thread(target=self._plex.download_from_url, args=(self._plex._account.username, self._plex._account.thumb))
            thread.daemon = True
            thread.start()

    def __on_ok_clicked(self, button):
        self.destroy()

    def __on_logout_clicked(self, button):
        self._plex.logout()
        self.destroy()
