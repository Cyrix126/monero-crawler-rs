 CREATE TABLE nodes (
   id INTEGER PRIMARY KEY,
   url TEXT NOT NULL,
   ping INTEGER NOT NULL,
   uptime INTEGER NOT NULL,
   last_checked INTEGER NOT NULL,
   zmq INTEGER,
   cors INTEGER 
 );
