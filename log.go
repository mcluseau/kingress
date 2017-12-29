package main

import (
	"bytes"
	"fmt"
	"log"
)

var (
	logCh = make(chan Loggable, 10)
)

type Loggable interface {
	ToLog(*LogMessage)
}

func processLog() {
	buf := &bytes.Buffer{}
	for loggable := range logCh {
		loggable.ToLog((*LogMessage)(buf))
		log.Print(buf.String())
		buf.Reset()
	}
}

type LogMessage bytes.Buffer

func (l *LogMessage) Field(name string, value interface{}) *LogMessage {
	buf := (*bytes.Buffer)(l)
	if buf.Len() != 0 {
		buf.WriteByte(' ')
	}

	switch value.(type) {
	case int, uint, int32, uint32, int64, uint64:
		fmt.Fprintf(buf, "%s=%d", name, value)

	default:
		fmt.Fprintf(buf, "%s=%q", name, value)
	}
	return l
}

func (l *LogMessage) Message(message string) *LogMessage {
	return l.Field("msg", message)
}

type LogField struct {
	Name  string
	Value interface{}
}
