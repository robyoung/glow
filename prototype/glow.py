import time
import datetime
from collections import namedtuple
import sys

import am2320
from typing import Optional, List, Tuple
from collections import namedtuple

import blinkt


LOOP_SLEEP_TIME = 30

Colour = namedtuple('Colour', 'red green blue')


class ColourRangeBucket:
    name: str
    value: float
    colour: Colour

    def __init__(self, name: str, value: float, colour: Colour):
        self.name = name
        self.value = value
        self.colour = colour


class ColourRange:
    buckets: List[ColourRangeBucket]
    num_pixels: int

    def __init__(
            self,
            buckets: Optional[List[ColourRangeBucket]]=None,
            num_pixels: int = blinkt.NUM_PIXELS,
    ):
        self.buckets = buckets or []
        self.num_pixels = num_pixels
        self._sort_buckets()

    def _sort_buckets(self):
        self.buckets.sort(key=lambda bucket: bucket.value)

    def get_pixels(self, value: float) -> List[Tuple[int, int, int]]:
        if value <= self.buckets[0].value:
            return [self.buckets[0].colour] * self.num_pixels
        elif value >= self.buckets[-1].value:
            return [self.buckets[-1].colour] * self.num_pixels
        else:
            for i in range(len(self.buckets) - 1):
                bottom, top = self.buckets[i], self.buckets[i + 1]
                if bottom.value <= value <= top.value:
                    bottom_to_value = value - bottom.value
                    bottom_to_top = top.value - bottom.value
                    num_pixels = round(self.num_pixels * bottom_to_value / bottom_to_top)

                    return [bottom.colour] * (self.num_pixels - num_pixels) + \
                            [top.colour] * num_pixels

        raise RuntimeError('cannot be here')


def stamp():
    return datetime.datetime.utcnow().isoformat()


def main():
    print('{},start'.format(stamp()))
    sensor = am2320.AM2320(1)

    previous_data = None
    error_count = 0

    colour_range = ColourRange(
        [
            ColourRangeBucket('blue', 14.0, Colour(10, 10, 226)),
            ColourRangeBucket('orange', 18.0, Colour(120, 20, 0)),
            ColourRangeBucket('salmon', 22.0, Colour(160, 10, 1)),
            ColourRangeBucket('coral', 26.0, Colour(255, 1, 1)),
            ColourRangeBucket('red', 30.0, Colour(255, 0, 100)),
        ]
    )

    while True:
        try:
            new_data = sensor.readSensor()
            error_count = 0
        except (OSError, Exception) as e:
            error_count += 1

            print(str(e), file=sys.stderr)
            print('Error reading sensor {} times'.format(error_count), file=sys.stderr)
            sys.stderr.flush()
            time.sleep(LOOP_SLEEP_TIME)
            if error_count > 3:
                print('Too many errors, exiting', file=sys.stderr)
                raise SystemExit(1)

        if previous_data is not None and new_data == previous_data:
            continue

        temperature, humidity = previous_data = new_data

        print(
            "{},data,{},{}".format(stamp(), temperature, humidity)
        )
        sys.stdout.flush()
        pixels = colour_range.get_pixels(temperature)

        for p, rgb in enumerate(pixels):
            blinkt.set_pixel(p, *rgb, brightness=0.05)
        blinkt.show()

        time.sleep(LOOP_SLEEP_TIME)

    print('{},end'.format(stamp()))


if __name__ == '__main__':
    main()
