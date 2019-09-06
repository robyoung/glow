import RPi.GPIO as GPIO

GPIO.setmode(GPIO.BCM)

GPIO.setup(17, GPIO.IN, pull_up_down=GPIO.PUD_UP)

# def my_callback(channel):
#     print('falling detect {}'.format(channel))
# 
# 
# GPIO.add_event_detect(17, GPIO.FALLING, callback=my_callback, bouncetime=300)
# GPIO.wait_for_edge(17, GPIO.RISING)
# GPIO.remove_event_detect(17)
GPIO.wait_for_edge(17, GPIO.RISING)
# GPIO.wait_for_edge(17, GPIO.FALLING)
print('fallen')
