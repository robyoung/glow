# Glow Web

The web component of [Glow](../). 

It is made up of an API for `glow-device` to communicate with, a web UI to display the
information and receive commands for the device and a couple of background monitors.

# Background monitors

- `EventsMonitor` periodically checks to see what the most recent event is so that we
  can alarm if the device has gone offline.
- `WeatherMonitor` polls a weather forecast and observation service. The idea is to
  correlate outside temperature changes with inside changes so we have a better idea of
  how cold the room is likely to get overnight.
