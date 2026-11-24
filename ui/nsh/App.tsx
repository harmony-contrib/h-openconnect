import { cleanup, mount } from "nsh";
import { HOpenConnectApp } from "./src/app";
import { CoreClient } from "./src/client";

mount(() => {
  const client = new CoreClient();
  cleanup(() => client.dispose());
  return <HOpenConnectApp client={client} />;
});
