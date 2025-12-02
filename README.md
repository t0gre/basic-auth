# Basic Auth

As Tony Hoare said; "There are two ways to build a system: You can either make it so simple there are obviously no flaws or you can make it so complex there are no obvious flaws".

Thesis: Auth has become way too complicated. Complexity is a risk in itself.

Solution: A very simple, fast, auth server that can handle lots of users. 

Features

 - [x] handle https requests
 - [x] threadpooling for requests
 - [x] login endpoint
 - [x] store passwords in db, encrypted
 - [x] roles
   -  [x] check requester role
 - [x] add/remove users if admin
 - [x] reset password
   - as user (with old password)
   - as admin (old user password not required) 


# How the api works

### admin add/reset:
check requestor is admin
assume original password is lost
upsert any user with the data provided (needs username, password, role)

example bodies
- 'bob:his_new_password'
- 'bob:his_new_password:ADMIN'


### user reset
use the password in body as new password
only for this user (only change password, not role)

example body 
- 'new_password' 

### admin delete:

check reqestor is admin
duh