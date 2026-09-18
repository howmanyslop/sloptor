export interface ImportedState {
  id: string;
}

declare const dependency: {
  createState(): ImportedState;
};

export default dependency;
