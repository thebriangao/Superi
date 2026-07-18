import {
  createContext,
  useContext,
  useMemo,
  type ReactNode,
} from "react";

import {
  createSuperiApiBindings,
  type SuperiApiBindings,
  type SuperiTransport,
} from "./api";

const SuperiApiContext = createContext<SuperiApiBindings | null>(null);

export interface SuperiApiProviderProps {
  readonly transport: SuperiTransport | null;
  readonly children: ReactNode;
}

export function SuperiApiProvider({
  transport,
  children,
}: SuperiApiProviderProps) {
  const api = useMemo(
    () => (transport === null ? null : createSuperiApiBindings(transport)),
    [transport],
  );

  return (
    <SuperiApiContext.Provider value={api}>
      {children}
    </SuperiApiContext.Provider>
  );
}

export function useSuperiApi(): SuperiApiBindings | null {
  return useContext(SuperiApiContext);
}
