# Glow

A hacky, connected Raspberry Pi alternative to the [Groegg](https://gro.co.uk/product/groegg-2/).
The LEDs change colour according to the Gro Company temperature bands.

## Hardware Components

- [AM2320](https://shop.pimoroni.com/products/digital-temperature-and-humidity-sensor) Digital temperature and Humidity sensor.
- [Blinkt](https://shop.pimoroni.com/products/blinkt) RGB LED strip.
- [Vibration sensor](https://thepihut.com/products/adafruit-medium-vibration-sensor-switch)
- [TPLink HS110](https://www.tp-link.com/uk/home-networking/smart-plug/hs110/) smart plug.

## Software Components

- [`glow-device`](./glow-device) runs on a Raspberry Pi and interacts with the hardware components, relays events to
  and receives commands from `glow-web`.
- [`glow-web`](./glow-web) is a web service that receives events from and sends commands to `glow-device`. It also
  presents a web UI.

## What does it do?

- It monitors the temperature and humidity in a room (my baby's nursery).
- Shows the current temperature as a blue / amber / red scale on an LED strip.
- Relays events such as environment measurements to `glow-web`.
- Controls a smart plug controlling a heater that can be controlled from `glow-web`.
