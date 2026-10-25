"""Explicit ML renderings for reference examples requiring layout or patterns."""

HAND_TWINS = {'functions/indexof.md': 'match indexOf ("foo=bar", "=")\n'
                         '    Success value => print value\n'
                         '    Error message => print message',
 'functions/jsonparse.md': 'match jsonParse "{\\"name\\": \\"osprey\\"}"\n'
                           '    Success value => print "parsed; release: ${jsonFree value}"\n'
                           '    Error message => print message',
 'functions/spawnprocess.md': 'onEvent (_pid, kind, data) = print "${kind}: ${trim data}"\n'
                              'match spawnProcess "echo hello" onEvent\n'
                              '    Success value =>\n'
                              '        exitCode = awaitProcess value\n'
                              '        cleanupProcess value\n'
                              '        print "exit: ${exitCode}"\n'
                              '    Error message => print "Process failed: ${message}"',
 'functions/structuralequals.md': 'type Route =\n'
                                  '    tab : string\n'
                                  '    depth : int\n'
                                  'same = structuralEquals (Route(tab = "home", depth = 1)) '
                                  '(Route(tab = "home", depth = 1))\n'
                                  'differ = structuralEquals [1, 2] [1, 2, 3]',
 'functions/yield.md': 'yield',
 'keywords/false.md': 'isComplete = false\n'
                      'match isComplete\n'
                      '    false => print "Not done yet"\n'
                      '    true => print "done"',
 'keywords/true.md': 'isReady = true\n'
                     'match isReady\n'
                     '    true => print "Ready!"\n'
                     '    false => print "Wait"',
 'keywords/type.md': 'type UserId =\n'
                     '    int\n'
                     '\n'
                     'type Status =\n'
                     '    Active\n'
                     '    Inactive\n'
                     '\n'
                     'type User =\n'
                     '    name : string\n'
                     '    age : int'}
