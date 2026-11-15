'use strict'

// An original reference application, written for the Osprey port's
// comparison and not taken from Express: JSON request bodies parsed by
// express.json() and validated, a Router mounted on /tasks, and a request
// header read with req.get. examples/json-api is its Osprey port, and every
// reply of the two is compared. [TS-PORT-EXPRESS-ORACLE]

var express = require('express')

var app = express()

app.use(express.json())

var notes = [{ id: 1, text: 'buy milk' }]

function error(status, message) {
  var err = new Error(message)
  err.status = status
  return err
}

app.get('/notes', function (req, res) {
  res.json(notes)
})

app.post('/notes', function (req, res, next) {
  var text = req.body === undefined || req.body === null ? undefined : req.body.text
  if (typeof text !== 'string' || text === '') {
    return next(error(422, 'text must be a non-empty string'))
  }
  res.status(201).json({ id: notes.length + 1, text: text })
})

var tasks = express.Router()

tasks.get('/', function (req, res) {
  res.json({ tasks: ['write', 'test'] })
})

tasks.get('/:id', function (req, res) {
  res.json({ id: req.params.id, agent: req.get('X-Agent') || 'unknown' })
})

app.use('/tasks', tasks)

app.use(function (err, req, res, next) {
  var malformed = err.type === 'entity.parse.failed'
  res.status(err.status || 500).json({ error: malformed ? 'malformed JSON' : err.message })
})

app.use(function (req, res) {
  res.status(404).json({ error: 'not found' })
})

app.listen(3000)
