; ModuleID = 'basic'
source_filename = "basic"

@json_root_0 = unnamed_addr constant [20 x i8] c"{\22message\22: \22hello\22}"
@buf_get_item = private global [256 x i8] zeroinitializer
@fmt_get_item_1 = unnamed_addr constant [33 x i8] c"{\22item_id\22: %lld, \22name\22: \22test\22}"

define void @root(ptr %0, ptr %1) {
entry:
  store ptr @json_root_0, ptr %0, align 8
  store i64 20, ptr %1, align 4
  ret void
}

define void @get_item(i64 %0, ptr %1, ptr %2) {
entry:
  %item_id = alloca i64, align 8
  store i64 %0, ptr %item_id, align 4
  %item_id1 = load i64, ptr %item_id, align 4
  %len = call i32 (ptr, i64, ptr, ...) @snprintf(ptr @buf_get_item, i64 256, ptr @fmt_get_item_1, i64 %item_id1)
  %len64 = sext i32 %len to i64
  store ptr @buf_get_item, ptr %1, align 8
  store i64 %len64, ptr %2, align 4
  ret void
}

declare i32 @snprintf(ptr, i64, ptr, ...)
