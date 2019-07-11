# original source https://gitlab.gnome.org/GNOME/gnome-music/blob/master/gnomemusic/mpris.py

from gi.repository import GLib, Gio


class Server:
    def __init__(self, con, path):
        method_outargs = {}
        method_inargs = {}
        for interface in Gio.DBusNodeInfo.new_for_xml(self.__doc__).interfaces:

            for method in interface.methods:
                method_outargs[method.name] = '(' + ''.join([arg.signature for arg in method.out_args]) + ')'
                method_inargs[method.name] = tuple(arg.signature for arg in method.in_args)

            con.register_object(object_path=path,
                                interface_info=interface,
                                method_call_closure=self.on_method_call)

        self.method_inargs = method_inargs
        self.method_outargs = method_outargs

    def on_method_call(self,
                       connection,
                       sender,
                       object_path,
                       interface_name,
                       method_name,
                       parameters,
                       invocation):

        args = list(parameters.unpack())
        for i, sig in enumerate(self.method_inargs[method_name]):
            if sig is 'h':
                msg = invocation.get_message()
                fd_list = msg.get_unix_fd_list()
                args[i] = fd_list.get(args[i])

        result = getattr(self, method_name)(*args)

        # out_args is atleast (signature1). We therefore always wrap the result
        # as a tuple. Refer to https://bugzilla.gnome.org/show_bug.cgi?id=765603
        result = (result,)

        out_args = self.method_outargs[method_name]
        if out_args != '()':
            variant = GLib.Variant(out_args, result)
            invocation.return_value(variant)
        else:
            invocation.return_value(None)


class MediaPlayer2Service(Server):
    '''
    <!DOCTYPE node PUBLIC '-//freedesktop//DTD D-BUS Object Introspection 1.0//EN'
    'http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd'>
    <node>
        <interface name='org.freedesktop.DBus.Introspectable'>
            <method name='Introspect'>
                <arg name='data' direction='out' type='s'/>
            </method>
        </interface>
        <interface name='org.freedesktop.DBus.Properties'>
            <method name='Get'>
                <arg name='interface' direction='in' type='s'/>
                <arg name='property' direction='in' type='s'/>
                <arg name='value' direction='out' type='v'/>
            </method>
            <method name="Set">
                <arg name="interface_name" direction="in" type="s"/>
                <arg name="property_name" direction="in" type="s"/>
                <arg name="value" direction="in" type="v"/>
            </method>
            <method name='GetAll'>
                <arg name='interface' direction='in' type='s'/>
                <arg name='properties' direction='out' type='a{sv}'/>
            </method>
        </interface>
        <interface name='org.mpris.MediaPlayer2'>
            <method name='Raise'>
            </method>
            <method name='Quit'>
            </method>
            <property name='CanQuit' type='b' access='read' />
            <property name='Fullscreen' type='b' access='readwrite' />
            <property name='CanRaise' type='b' access='read' />
            <property name='HasTrackList' type='b' access='read'/>
            <property name='Identity' type='s' access='read'/>
            <property name='DesktopEntry' type='s' access='read'/>
            <property name='SupportedUriSchemes' type='as' access='read'/>
            <property name='SupportedMimeTypes' type='as' access='read'/>
        </interface>
        <interface name='org.mpris.MediaPlayer2.Player'>
            <method name='Next'/>
            <method name='Previous'/>
            <method name='Pause'/>
            <method name='PlayPause'/>
            <method name='Stop'/>
            <method name='Play'/>
            <method name='Seek'>
                <arg direction='in' name='Offset' type='x'/>
            </method>
            <method name='SetPosition'>
                <arg direction='in' name='TrackId' type='o'/>
                <arg direction='in' name='Position' type='x'/>
            </method>
            <method name='OpenUri'>
                <arg direction='in' name='Uri' type='s'/>
            </method>
            <signal name='Seeked'>
                <arg name='Position' type='x'/>
            </signal>
            <property name='PlaybackStatus' type='s' access='read'/>
            <property name='LoopStatus' type='s' access='readwrite'/>
            <property name='Rate' type='d' access='readwrite'/>
            <property name='Shuffle' type='b' access='readwrite'/>
            <property name='Metadata' type='a{sv}' access='read'>
            </property>
            <property name='Position' type='x' access='read'/>
            <property name='MinimumRate' type='d' access='read'/>
            <property name='MaximumRate' type='d' access='read'/>
            <property name='CanGoNext' type='b' access='read'/>
            <property name='CanGoPrevious' type='b' access='read'/>
            <property name='CanPlay' type='b' access='read'/>
            <property name='CanPause' type='b' access='read'/>
            <property name='CanSeek' type='b' access='read'/>
            <property name='CanControl' type='b' access='read'/>
        </interface>
    </node>
    '''

    MEDIA_PLAYER2_IFACE = 'org.mpris.MediaPlayer2'
    MEDIA_PLAYER2_PLAYER_IFACE = 'org.mpris.MediaPlayer2.Player'

    def __repr__(self):
        return '<MediaPlayer2Service>'

    def __init__(self, app):
        self.con = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        Gio.bus_own_name_on_connection(self.con,
                                       'org.mpris.MediaPlayer2.Girens',
                                       Gio.BusNameOwnerFlags.NONE,
                                       None,
                                       None)
        super().__init__(self.con, '/org/mpris/MediaPlayer2')

        self.app = app
        self.player = app._player

        self.player.connect('media-playing', self._on_current_song_changed)
        self.player.connect('media-paused', self._on_player_state_changed)
        self.player.connect("media-time", self.__on_media_time)



        self._previous_playback_status = "Stopped"

    def _get_metadata(self, media=None, index=None):
        song_dbus_path = self._get_song_dbus_path(media, index)
        if not self.player._item:
            return {
                'mpris:trackid': GLib.Variant('o', song_dbus_path)
            }

        if media is None:
            media = self.player._item


        length = media.duration * 1000
        if media.type is not 'track':
            user_rating = 1.0
        else:
            user_rating = media.userRating / 10

        if media.type in {'movie', 'clip'}:
            index = 0
            album = media.title
            artist = media.title
        else:
            index = self.player._item.index
            album = media.parentTitle
            artist = media.grandparentTitle



        metadata = {
            'mpris:trackid': GLib.Variant('o', song_dbus_path),
            'mpris:length': GLib.Variant('x', length),
            'xesam:trackNumber': GLib.Variant('i', int(index)),
            'xesam:useCount': GLib.Variant('i', media.viewCount),
            'xesam:userRating': GLib.Variant('d', user_rating),
            'xesam:title': GLib.Variant('s', media.title),
            'xesam:album': GLib.Variant('s', album),
            'xesam:artist': GLib.Variant('as', [artist]),
            'xesam:albumArtist': GLib.Variant('as', [artist])
        }

        last_played = media.viewedAt
        if last_played is not None:
            last_played_str = last_played.format("%FT%T%:z")
            metadata['xesam:lastUsed'] = GLib.Variant('s', last_played_str)

        path_image = self.app._plex.path_for_download('thumb_' + str(media.ratingKey))[1]
        if (path_image is not None):
            metadata['mpris:artUrl'] = GLib.Variant('s', "file://" + path_image)

        return metadata

    def _get_song_dbus_path(self, media=None, index=None):
        """Convert a Grilo media to a D-Bus path

        The hex encoding is used to remove any possible invalid character.
        Use player index to make the path truly unique in case the same song
        is present multiple times in a playlist.
        If media is None, it means that the current song path is requested.

        :param Grl.Media media: The media object
        :param int index: The media position in the current playlist
        :return: a D-Bus id to uniquely identify the song
        :rtype: str
        """
        if not self.player._item:
            return "/org/mpris/MediaPlayer2/TrackList/NoTrack"

        if not media:
            media = self.player._item
            if media.type == 'movie':
                index = 0
            else:
                index = self.player._item.index

        id_hex = media.key.encode('ascii').hex()
        path = "/nl/g4d/Girens/TrackList/{}_{}".format(
            id_hex, index)
        return path

    def __on_media_time(self, player, time):
        self.Seeked(time * 1000)


    def _on_current_song_changed(self, player, playing, playqueue_item, playqueue, offset, item):
        properties = {}
        properties["Metadata"] = GLib.Variant("a{sv}", self._get_metadata(media=player._item))
        properties["CanGoNext"] = GLib.Variant("b", True)
        properties["CanGoPrevious"] = GLib.Variant("b", True)
        properties["CanPause"] = GLib.Variant("b", True)
        properties["CanPlay"] = GLib.Variant("b", True)

        self.PropertiesChanged(
            MediaPlayer2Service.MEDIA_PLAYER2_PLAYER_IFACE, properties, [])

    def _on_player_state_changed(self, klass, args):
        playback_status = self._get_playback_status()
        if playback_status == self._previous_playback_status:
            return

        self._previous_playback_status = playback_status
        self.PropertiesChanged(MediaPlayer2Service.MEDIA_PLAYER2_PLAYER_IFACE,
                               {
                                   'PlaybackStatus': GLib.Variant('s', playback_status),
                               },
                               [])

    def Raise(self):
        self.app.do_activate()

    def Quit(self):
        self.app.quit()

    def Next(self):
        self.player.next()

    def Previous(self):
        self.player.prev()

    def Pause(self):
        self.player.pause()

    def PlayPause(self):
        self.player.play_pause()

    def Stop(self):
        self.player.stop()

    def Play(self):
        """Start or resume playback.

        If there is no track to play, this has no effect.
        """
        self.player.play()

    def Seek(self, offset_msecond):
        pass

    def SetPosition(self, track_id, position_msecond):
        pass

    def OpenUri(self, uri):
        pass

    def Get(self, interface_name, property_name):
        return self.GetAll(interface_name)[property_name]

    def GetAll(self, interface_name):
        if interface_name == MediaPlayer2Service.MEDIA_PLAYER2_IFACE:
            application_id = self.app._aplication.get_application_id()
            return {
                'CanQuit': GLib.Variant('b', True),
                'Fullscreen': GLib.Variant('b', False),
                'CanSetFullscreen': GLib.Variant('b', False),
                'CanRaise': GLib.Variant('b', True),
                'HasTrackList': GLib.Variant('b', False),
                'Identity': GLib.Variant('s', 'Girens'),
                'DesktopEntry': GLib.Variant('s', application_id),
                'SupportedUriSchemes': GLib.Variant('as', [
                    'file'
                ]),
                'SupportedMimeTypes': GLib.Variant('as', [
                    'application/ogg',
                    'audio/x-vorbis+ogg',
                    'audio/x-flac',
                    'audio/mpeg'
                ]),
            }
        elif interface_name == MediaPlayer2Service.MEDIA_PLAYER2_PLAYER_IFACE:
            position_msecond = 0
            if self.player._progresNow is not None:
                position_msecond = self.player._progresNow * 1e6
            return {
                'PlaybackStatus': GLib.Variant('s', self._get_playback_status()),
                'Rate': GLib.Variant('d', 1.0),
                'Metadata': GLib.Variant('a{sv}', self._get_metadata()),
                'Position': GLib.Variant('x', position_msecond),
                'MinimumRate': GLib.Variant('d', 1.0),
                'MaximumRate': GLib.Variant('d', 1.0),
                'CanGoNext': GLib.Variant('b', True),
                'CanGoPrevious': GLib.Variant('b', True),
                'CanPlay': GLib.Variant('b', self.player._item is not None),
                'CanPause': GLib.Variant('b', self.player._item is not None),
                'CanSeek': GLib.Variant('b', True),
                'CanControl': GLib.Variant('b', True),
                'Volume': GLib.Variant('d', 1.0)
            }
        elif interface_name == 'org.freedesktop.DBus.Properties':
            return {}
        elif interface_name == 'org.freedesktop.DBus.Introspectable':
            return {}
        else:
            logger.warning(
                "MPRIS does not implement {} interface".format(interface_name))

    def _get_playback_status(self):
        if self.player._item is not None:
            return self.player.get_state()
        else:
            return "Stopped"


    def Set(self, interface_name, property_name, new_value):
        if interface_name == MediaPlayer2Service.MEDIA_PLAYER2_IFACE:
            if property_name == 'Fullscreen':
                pass
        elif interface_name == MediaPlayer2Service.MEDIA_PLAYER2_PLAYER_IFACE:
            if property_name in ['Rate', 'Volume']:
                pass
            elif property_name == 'LoopStatus':
                if new_value == 'None':
                    self.player.props.repeat_mode = RepeatMode.NONE
                elif new_value == 'Track':
                    self.player.props.repeat_mode = RepeatMode.SONG
                elif new_value == 'Playlist':
                    self.player.props.repeat_mode = RepeatMode.ALL
            elif property_name == 'Shuffle':
                if new_value:
                    self.player.props.repeat_mode = RepeatMode.SHUFFLE
                else:
                    self.player.props.repeat_mode = RepeatMode.NONE
        else:
            logger.warning(
                "MPRIS does not implement {} interface".format(interface_name))

    def Seeked(self, position_msecond):
        """Indicate that the track position has changed.

        :param int position_msecond: new position in microseconds.
        """
        variant = GLib.Variant.new_tuple(GLib.Variant('x', position_msecond))
        self.con.emit_signal(
            None, '/org/mpris/MediaPlayer2',
            MediaPlayer2Service.MEDIA_PLAYER2_PLAYER_IFACE, 'Seeked', variant)


    def PropertiesChanged(self, interface_name, changed_properties,
                          invalidated_properties):
        self.con.emit_signal(None,
                             '/org/mpris/MediaPlayer2',
                             'org.freedesktop.DBus.Properties',
                             'PropertiesChanged',
                             GLib.Variant.new_tuple(GLib.Variant('s', interface_name),
                                                    GLib.Variant('a{sv}', changed_properties),
                                                    GLib.Variant('as', invalidated_properties)))

    def Introspect(self):
        return self.__doc__
