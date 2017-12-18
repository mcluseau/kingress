package config

import (
	"flag"
	"time"
)

var (
	changeApplyDelay = flag.Duration("change-apply-delay", 100*time.Millisecond, "Delay before applying change in Kubernetes configuration")

	changeNum        uint64 = 0
	appliedChangeNum uint64 = 0
)

type NewConfigFunc func() Config

func NotifyChanged(callback NewConfigFunc) {
	changeNum += 1
	go applyChange(changeNum, callback)
}

// Wait a bit and apply the changes
func applyChange(myChangeNum uint64, callback NewConfigFunc) {
	time.Sleep(*changeApplyDelay)

	if appliedChangeNum >= myChangeNum {
		return // already applied
	}

	Lock()
	defer Unlock()

	if appliedChangeNum >= myChangeNum {
		return // already applied
	}

	config := callback()
	Current = &config

	appliedChangeNum = changeNum
}
