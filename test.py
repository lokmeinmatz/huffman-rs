from textwrap import wrap

binstr = "1"
hexstr = ""

for i in range(1, 1000):
    a = '{message:{fill}{align}{width}}'.format(message=bin(i%256)[2:], fill='0', align='>', width=8)
    if i % 2 == 0:
        a += "1"
    else:
        a += "0"
    binstr += a

binstr += "0" * (8 - (len(binstr) % 8))
print("es wurden {} 0en angefügt".format(8 - (len(binstr) % 8)))

for part in wrap(binstr, 8):
    hexstr += '{data:0>2}'.format(data=hex(int(part, 2))[2:])

#print(binstr[:100])
for line in wrap(hexstr[:300], 32):
    print(" ".join(wrap(line, 2)))