#!/usr/bin/env python3
"""Create a private NVS CSV interactively, without compiling credentials into firmware."""
import csv
import getpass
import os
import pathlib
import re
import sys

def main():
    destination = pathlib.Path(sys.argv[1] if len(sys.argv)>1 else 'runtime/nvs.secret')
    destination.parent.mkdir(mode=0o700,parents=True,exist_ok=True)
    ssid = input('Wi-Fi SSID: ')
    password = getpass.getpass('Wi-Fi password: ')
    base = input('HTTPS origin (no trailing slash, DNS host, optional port): ')
    device = input('New device ID (1..24 lowercase letters/digits/hyphens): ')
    token = getpass.getpass('Device token (64 hex characters matching server config): ')
    if not (1 <= len(ssid.encode()) <= 32 and 8 <= len(password.encode()) <= 63 and
            re.fullmatch(r'https://[a-zA-Z0-9.-]+(?::[0-9]{1,5})?',base) and len(base)<=160 and
            re.fullmatch(r'[a-z0-9-]{1,24}',device) and re.fullmatch(r'[a-fA-F0-9]{64}',token)):
        raise SystemExit('Invalid provisioning input')
    with os.fdopen(os.open(destination,os.O_CREAT|os.O_EXCL|os.O_WRONLY,0o600),'w',newline='') as f:
        w=csv.writer(f); w.writerow(['key','type','encoding','value']); w.writerow(['xlens','namespace','',''])
        for k,v in [('ssid',ssid),('password',password),('base',base),('id',device),('token',token)]:
            w.writerow([k,'data','string',v])
        w.writerow(['reserved','data','u64','0'])
    print('Created private NVS input. Do not reset an existing device counter; use a new device ID/token when reprovisioning.')
if __name__=='__main__': main()
