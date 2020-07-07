# Glow Device

The Raspberry Pi device component of [Glow](../).

It is made up of an event loop that interacts with `MessageHandler`s. `MessageHandler`s
generally fall into two categories; Sensors listen to a some external component such as
a hardware sensor or something on the network and emit events onto the bus while Handlers
listen for events and react to them, potentially emiting more events onto the bus.

- `EnvironmentSensor` reads the AM2320 temperature and humidity sensor.
- `VibrationSensor` translates interrupts from the vibration sensor into tap events.
- `LEDHandler` controls the Blinkt colour LED strip.
- `TPLinkHandler` controls the TPLink smart switch.
- `WebEventHandler` receives commands from `glow-web` and relays events back to it.
