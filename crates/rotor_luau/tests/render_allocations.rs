//! Port of `TestTempIdsRepeatedHintsAllocations` from
//! `internal/luau/render/solvetempids_test.go`. Counting allocations needs a
//! global allocator, so it lives in its own test binary.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use regex as _;
use rotor_luau::render::render_ast;
use rotor_luau::{Ast, List, element};

struct Counting;

thread_local! {
	static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

fn count() {
	let _ = ALLOCATIONS.try_with(|allocations| allocations.set(allocations.get() + 1));
}

#[expect(unsafe_code, reason = "counting allocations means implementing GlobalAlloc")]
// SAFETY: every method forwards unchanged to the system allocator.
unsafe impl GlobalAlloc for Counting {
	unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
		count();
		// SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract.
		unsafe { System.alloc(layout) }
	}

	unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
		// SAFETY: the caller upholds `GlobalAlloc::dealloc`'s contract.
		unsafe { System.dealloc(ptr, layout) }
	}

	unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
		count();
		// SAFETY: the caller upholds `GlobalAlloc::realloc`'s contract.
		unsafe { System.realloc(ptr, layout, new_size) }
	}
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn allocations() -> usize {
	ALLOCATIONS.with(Cell::get)
}

fn repeated_temp_statements(ast: &mut Ast, count: usize) -> List<element::Statement> {
	let statements = ast.new_list();
	for _ in 0..count {
		let (temp, one) = (ast.temp_id("value"), ast.num(1.0));
		let declaration = ast.variable_declaration(temp, Some(one.into()));
		ast.list_push(statements, declaration);
	}
	statements
}

#[test]
fn temp_ids_repeated_hints_allocations() {
	let mut ast = Ast::new();
	let statements = repeated_temp_statements(&mut ast, 1000);
	// A thousand same-hint temps once allocated about 900,000 objects by
	// repeatedly formatting occupied suffixes. Allow ample renderer overhead
	// while ensuring that collision probing stays within a linear budget.
	render_ast(&ast, statements).expect("warm-up render");
	let before = allocations();
	render_ast(&ast, statements).expect("render");
	let used = allocations() - before;
	assert!(used > 0, "the counting allocator saw no allocations");
	assert!(
		used <= 20_000,
		"1000 repeated hints allocated {used} objects, want at most 20000"
	);
}
