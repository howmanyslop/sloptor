function ReactComponent(ctor: defined) {
	print("ReactComponent", ctor);
}

function LogF(tag: string) {
	return (target: defined, key: string) => {
		print("LogF", tag, target, key);
	};
}

function LogMF(tag: string) {
	return (target: defined, key: string, descriptor: { value: unknown }) => {
		print("LogMF", tag, target, key, descriptor);
	};
}

const ns = {
	deco: (target: defined, key: string) => {
		print("ns.deco", target, key);
	},
};

interface ErrorBoundaryProps {
	fallback: string;
}
interface ErrorBoundaryState {
	hasError: boolean;
}

class Component<P, S> {
	props!: P;
	state!: S;
	setState(s: S) {
		this.state = s;
	}
}

const React = { Component: Component };

@ReactComponent
export class ErrorBoundary extends React.Component<ErrorBoundaryProps, ErrorBoundaryState> {
	@LogF("state")
	currentState = { hasError: false };

	@ns.deco
	other = 1;

	@LogMF("derived")
	componentDidCatch(message: unknown) {
		this.setState({ hasError: true });
		print(message);
	}
}

print(ErrorBoundary);
