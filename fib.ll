; ModuleID = 'fib.7f8e2dee27358b02-cgu.0'
source_filename = "fib.7f8e2dee27358b02-cgu.0"
target datalayout = "e-m:o-i64:64-i128:128-n32:64-S128"
target triple = "arm64-apple-macosx11.0.0"

; Function Attrs: nofree norecurse nosync nounwind memory(none) uwtable
define noundef i32 @fib(i32 noundef %a, i32 noundef %b, i32 noundef %n) unnamed_addr #0 {
start:
  %0 = icmp eq i32 %n, 0
  br i1 %0, label %bb4, label %bb2

bb2:                                              ; preds = %start, %bb2
  %n.tr3 = phi i32 [ %_5, %bb2 ], [ %n, %start ]
  %b.tr2 = phi i32 [ %_4, %bb2 ], [ %b, %start ]
  %a.tr1 = phi i32 [ %b.tr2, %bb2 ], [ %a, %start ]
  %_4 = add i32 %b.tr2, %a.tr1
  %_5 = add i32 %n.tr3, -1
  %1 = icmp eq i32 %_5, 0
  br i1 %1, label %bb4, label %bb2

bb4:                                              ; preds = %bb2, %start
  %b.tr.lcssa = phi i32 [ %b, %start ], [ %_4, %bb2 ]
  ret i32 %b.tr.lcssa
}

attributes #0 = { nofree norecurse nosync nounwind memory(none) uwtable "frame-pointer"="non-leaf" "probe-stack"="inline-asm" "target-cpu"="apple-m1" }

!llvm.module.flags = !{!0}
!llvm.ident = !{!1}

!0 = !{i32 8, !"PIC Level", i32 2}
!1 = !{!"rustc version 1.79.0 (129f3b996 2024-06-10)"}
