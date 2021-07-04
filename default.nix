let 
  holonixPath = builtins.fetchTarball {
    url = "https://github.com/holochain/holonix/archive/014d28000c8ed021eb84000edfe260c22e90af8c.tar.gz";
    sha256 = "0hl5xxxjg2a6ymr44rf5dfvsb0c33dq4s6vibva6yb76yvl6gwfi";
  };
  holonix = import (holonixPath) {
    includeHolochainBinaries = true;
    holochainVersionId = "custom";
    
    holochainVersion = { 
     rev = "3dc2d87f7f6de66d7de2c9160b6a962331ddd926";
     sha256 = "0i4b39mm45jbwdw70brj43h8ga9lcsjnbbqhss0cqdqqn8dpsky6";
     cargoSha256 = "0gca0mg20ix61ps1lngzvn9cjvzylbpql36p6zr6cwp3k78dbpkl";
     bins = {
       holochain = "holochain";
       hc = "hc";
     };
    };
    holochainOtherDepsNames = ["lair-keystore"];
  };
in holonix.main