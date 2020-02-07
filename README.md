# Glow

A hacky Raspberry Pi alternative to the [Groegg](https://gro.co.uk/product/groegg-2/).
The LEDs change colour according to the Gro Company temperature bands.

## Electronic Components

- [AM2320](https://shop.pimoroni.com/products/digital-temperature-and-humidity-sensor) Digital temperature and Humidity sensor.
- [Blinkt](https://shop.pimoroni.com/products/blinkt) RGB LED strip.
- [Vibration sensor](https://thepihut.com/products/adafruit-medium-vibration-sensor-switch)
- [Nokia 5110 LCD screen](https://thepihut.com/products/adafruit-nokia-5110-3310-monochrome-lcd-extras)


## Software Components
- `glow-device` interact with electrical components and relay events to web service
- `glow-web` web service that receives events, makes decisions and sends control events

## Planned features

### Send measurements to cloud - DONE

For a simple entrypoint to other things a first stab could be a IFTTT web hook.

### Control smart switch

This should be strictly controlled to avoid the risk of the heater being left on too
long and over heating the room.

- Glow device cannot control the heater directly.
- Glow device sends environment sensor readings to glow web which records them.
- If glow web stops receiving sensor readings it should alarm via text message.
- Glow web can decide to switch on the heater. This is done with a single message that switches on the heater for 
  a set amount of time. Glow web does not need to explicitly switch the heater off, this is done by glow device.
- If glow web either detects that the heater has not been switched off or that it is no longer receiving
  sensor readings it should alarm via text message.

### Show status on LCD display

Each iteration update the LCD screen to show temperature, humidity, date and time. The backlight is
controlled by the illumination state introduced in phase 2.

### Show high / low metrics

Show the high and low values for temperature and humidity over past 1 day, 7 days and 30 days.

This iteration may require different tap gestures to change LCD display. Maybe single tap to change
illumination and double tap to change LCD display.

