#include "nestedloop.h"
#include "annotationsearch.h"

using namespace annis;


NestedLoopJoin::NestedLoopJoin(std::shared_ptr<Operator> op,
                               std::shared_ptr<AnnoIt> lhs,
                               std::shared_ptr<AnnoIt> rhs)
  : op(op), initialized(false),
    left(lhs), right(rhs)
{
}

BinaryMatch NestedLoopJoin::next()
{
  BinaryMatch result;
  result.found = false;

  if(!op || !left || !right)
  {
    return result;
  }

  bool proceed = true;

  if(!initialized)
  {
    proceed = false;
    if(left->hasNext())
    {
      matchLeft = left->next();
      proceed = true;
      initialized = true;
    }
  }

  while(proceed)
  {

    while(right->hasNext())
    {
      matchRight = right->next();

      if(op->filter(matchLeft, matchRight))
      {
        result.found = true;
        result.lhs = matchLeft;
        result.rhs = matchRight;

        return result;
      }
    }
    if(left->hasNext())
    {
      matchLeft = left->next();
      right->reset();
    }
    else
    {
      proceed = false;
    }
  }
  return result;
}

void NestedLoopJoin::reset()
{
  left->reset();
  right->reset();
  initialized = false;
}

NestedLoopJoin::~NestedLoopJoin()
{

}
