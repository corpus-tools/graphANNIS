/*
   Copyright 2017 Thomas Krause <thomaskrause@posteo.de>

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
*/

#pragma once

#include <annis/operators/operator.h>  // for Operator
#include <annis/util/helper.h>         // for TokenHelper
#include <memory>                      // for shared_ptr, unique_ptr
#include <string>                      // for string
#include <annis/types.h>               // for Match (ptr only), Annotation
#include <annis/db.h>
namespace annis { class AnnoIt; }
namespace annis { class ReadableGraphStorage; }

namespace annis
{

class IdenticalCoverage : public Operator
{
public:
  IdenticalCoverage(const DB &db, DB::GetGSFuncT getGraphStorageFunc);
  IdenticalCoverage(const IdenticalCoverage& orig) = delete;
  
  virtual std::unique_ptr<AnnoIt> retrieveMatches(const Match& lhs) override;
  virtual bool filter(const Match& lhs, const Match& rhs) override;
  virtual bool isReflexive() override {return false;}
  virtual bool isCommutative() override {return true;}

  virtual std::string description() override
  {
    return "_=_";
  }

  virtual double selectivity() override;

  
  virtual ~IdenticalCoverage();
private:
  
  TokenHelper tokHelper;
  std::shared_ptr<const ReadableGraphStorage> gsOrder;
  std::shared_ptr<const ReadableGraphStorage> gsLeftToken;
  std::shared_ptr<const ReadableGraphStorage> gsRightToken;
  std::shared_ptr<const ReadableGraphStorage> gsCoverage;
  
  Annotation anyNodeAnno;

};

} // end namespace annis


