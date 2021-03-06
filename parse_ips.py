#!/usr/bin/python
import socket

def int_to_ip(ip):
    return socket.inet_ntoa(hex(ip)[2:].zfill(8).decode('hex'))

dir = "GeoLiteCity_20131001/"
blocks = "GeoLiteCity-Blocks.csv"
cities = "GeoLiteCity-Location.csv"


#returns a set of Id's matching the city we are looking for
def find_city(city_name):
    matches = set()
    fd = open(dir+cities)
    fd.readline()
    for line in fd:
        split = line.strip().split(",")
        if split[3].replace("\"", "") == city_name:
            matches.add(split[0])
    return matches


# takes a set of city Ids and returns 
def find_ip_ranges(ids):
    fd = open(dir+blocks)
    fd.readline()
    fd.readline()
    ranges = []
    for line in fd:
        split = line.strip().replace("\"", "").split(",")
        if split[2] in ids:
            ranges.append((int(split[0]), int(split[1])))
    return ranges

if __name__ == "__main__":
    import sys
    if len(sys.argv) != 2:
        print "Usage: ./parse_ips.py [Name of city]"

    city_map = open(dir + blocks)

    city_map.readline()
    for i in range(100000):
        city_map.readline()
    city_map.readline()
    s =  city_map.readline().strip()
    #print s

    start_ip, end_ip, loc_id = s.replace("\"", "").split(",")

    #print int_to_ip(int(start_ip)), int_to_ip(int(end_ip))

       
    matches = find_city(sys.argv[1])
    #print matches
    ips_range = find_ip_ranges(matches)

    for tup in ips_range:
        print tup[0], tup[1]
    print 0, 0
    print 2130706433, 2130706433 #127.0.0.1
    #print int_to_ip(tup[0]), "\t--\t", int_to_ip(tup[1])
