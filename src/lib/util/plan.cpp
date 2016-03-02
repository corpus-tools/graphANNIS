/*
 * To change this license header, choose License Headers in Project Properties.
 * To change this template file, choose Tools | Templates
 * and open the template in the editor.
 */

/* 
 * File:   Plan.cpp
 * Author: thomas
 * 
 * Created on 1. März 2016, 11:48
 */

#include <annis/util/plan.h>

using namespace annis;

Plan::Plan(const std::vector<std::shared_ptr<AnnoIt>>& source)
: source(source), cost(-1.0)
{
}

Plan::Plan(const Plan& orig)
{
  source = orig.source;
  cost = orig.cost;
}

bool Plan::executeStep(std::vector<Match>& result)
{
  if(source.size() == 0 || result.size() != source.size())
  {
    return false;
  }
  
  for(size_t i=0; i < source.size(); i++)
  {
    if(!source[i]->next(result[i]))
    {
      return false;
    }
  }
  return true;
}


double Plan::getCost() {
  if(cost < 0.0)
  {
    // TODO: calculate the cost
  }
  
  return cost;
}


Plan::~Plan()
{
}

