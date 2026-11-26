# Pasting on Ubuntu GNOME Wayland

When Grain transcribes successfully but the text does not reach the focused app, test the paste path separately from speech recognition.

1. Confirm the desktop session with `echo "$XDG_SESSION_TYPE"`. For Wayland clipboard support, install `wl-clipboard` if `wl-copy` is missing.
2. If the recording overlay takes focus, set **Settings → Application → Recording pill** to **None** and test in a text editor.
3. For simulated typing, install `ydotool` and make sure its daemon can access `/dev/uinput`. Use the permissions and service setup provided by your distribution. Test `ydotool type 'Grain paste test'` with the text editor focused before changing Grain settings.
4. In **Settings → Output**, set **Paste Method** to **Direct**, then select **ydotool** under **Typing Tool** if it appears. The selector lists tools detected on the system.
5. If the global shortcut itself fails, check **Keyboard Implementation** under **Settings → Application → Experimental**. After switching backends, verify the shortcut assignment.

Some simulated clipboard shortcuts use physical key positions. If they fail on a non-QWERTY layout, try **Direct** or configure **External Script** under **Paste Method**. Grain passes the transcript to the script as its first argument.
