import gi
import mpv
import threading

gi.require_version('Gtk', '3.0')
from gi.repository import Gtk, GdkPixbuf

# import gi
# gi.require_version('Gtk', '3.0')
# locale.setlocale(locale.LC_NUMERIC, 'C')
# player = mpv.MPV()

class PlayerThread:
    def __init__(self, playqueue):
        self.player = None
        self.progresUpdate = None
        self.progresNow = None
        self.item = None
        self.playqueue = playqueue

    def createPlayer(self):
        import locale
        locale.setlocale(locale.LC_NUMERIC, 'C')
        self.player = mpv.MPV(input_default_bindings=True, input_vo_keyboard=True, vo='x11', title=self.item.title)

        @self.player.property_observer('time-pos')
        def time_observer(_name, value):
            # Here, _value is either None if nothing is playing or a float containing
            # fractional seconds since the beginning of the file.
            # print('Now playing at {:.2f}s'.format(value))
            # print(value)
            if value is not None and abs(value - self.progresUpdate) > 5:
                self.progresUpdate = value
                self.item.updateTimeline(value * 1000, state='playing', duration=self.item.duration)
            pass

        @self.player.property_observer('eof-reached')
        def time_observer(_name, value):
            print(_name, value)

        @self.player.on_key_press('STOP')
        def my_close_binding():
            print('Stop press')

        @self.player.on_key_press('z')
        def my_z_binding():
            print('z')
            # self.player.seek(10, reference="absolute-percent")
            self.player.command('seek', 10, "absolute-percent")
            print('y')

    def stop(self):
        print('stopped')
        self.player.terminate()
        self.item.updateTimeline(self.progresNow * 1000, state='stopped', duration=self.item.duration)
        import locale
        locale.setlocale(locale.LC_NUMERIC, 'C')
        self.createPlayer()

    def play(self):
        self.item = self.playqueue.items[int(self.playqueue.playQueueSelectedItemOffset)]
        self.createPlayer()
        self.progresUpdate = 0
        self.progresNow = 0
        print(self.item.viewOffset)
        self.player.play(self.item.getStreamURL(offset=self.item.viewOffset / 1000))
        # self.player.seek(self.item.viewOffset / 1000, reference="absolute")
        # self.player.command('seek', 10, "absolute-percent")
        self.player.wait_for_playback()
        self.stop()



class Player():
    def __init__(self, playqueue):
        player = PlayerThread(playqueue)
        player.play()


